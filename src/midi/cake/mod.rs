use std::{path::PathBuf, sync::Arc, thread};
use time::Duration;

use midi_toolkit::{
    events::{Event, MIDIEventEnum},
    io::MIDIFile as TKMIDIFile,
    pipe,
    sequence::{
        event::{cancel_tempo_events, scale_event_time, Delta, EventBatch, Track},
        unwrap_items, TimeCaster,
    },
};

use crate::{
    audio_playback::WasabiAudioPlayer,
    gui::window::WasabiError,
    midi::{
        audio::ram::InRamAudioPlayer,
        cake::tree_threader::{NoteEvent, ThreadedTreeSerializers},
        open_file_and_signature,
        shared::{audio::CompressedAudio, timer::TimeKeeper},
        MIDIColor,
    },
    settings::MidiSettings,
};

use self::blocks::CakeBlock;

use super::{MIDIFileBase, MIDIFileStats, MIDIFileUniqueSignature};

pub mod blocks;
pub mod intvec4;
mod tree_serializer;
mod tree_threader;
mod unended_note_batch;

pub struct CakeMIDIFile {
    blocks: Vec<CakeBlock>,
    timer: TimeKeeper,
    length: f64,
    note_count: u64,
    ticks_per_second: u32,
    signature: MIDIFileUniqueSignature,
}

impl CakeMIDIFile {
    pub fn load_from_file(
        path: impl Into<PathBuf>,
        player: Arc<WasabiAudioPlayer>,
        settings: &MidiSettings,
    ) -> Result<Self, WasabiError> {
        let ticks_per_second = 10000;

        let (file, signature) = open_file_and_signature(path)?;
        let midi = TKMIDIFile::open_from_stream(file, None).map_err(WasabiError::MidiLoadError)?;

        let ppq = midi.ppq();
        let merged = pipe!(
            midi.iter_all_track_events_merged_batches()
            |>TimeCaster::<f64>::cast_event_delta()
            |>cancel_tempo_events(250000)
            |>scale_event_time(1.0 / ppq as f64)
            |>unwrap_items()
        );

        let colors = MIDIColor::new_vec_from_settings(midi.track_count(), settings)?;

        type Ev = Delta<f64, Track<EventBatch<Event>>>;
        let (key_snd, key_rcv) = crossbeam_channel::bounded::<Arc<Ev>>(1000);
        let (audio_snd, audio_rcv) = crossbeam_channel::bounded::<Arc<Ev>>(1000);

        let key_join_handle = thread::spawn(move || {
            let mut trees = ThreadedTreeSerializers::new();

            let mut time = 0.0;

            let mut note_count = 0;

            for batch in key_rcv.into_iter() {
                time += batch.delta;

                let int_time = (time * ticks_per_second as f64) as i32;

                fn channel_track(channel: u8, track: u32) -> i32 {
                    (channel as i32) + (track as i32) * 16
                }

                for event in batch.iter_events() {
                    let track = event.track;
                    match event.as_event() {
                        Event::NoteOn(e) => {
                            let channel_track = channel_track(e.channel, track);

                            trees.push_event(
                                e.key as usize,
                                NoteEvent::On {
                                    time: int_time,
                                    channel_track,
                                    color: colors[channel_track as usize].as_u32() as i32,
                                },
                            );
                            note_count += 1;
                        }
                        Event::NoteOff(e) => {
                            let channel_track = channel_track(e.channel, track);

                            trees.push_event(
                                e.key as usize,
                                NoteEvent::Off {
                                    time: int_time,
                                    channel_track,
                                    color: colors[channel_track as usize].as_u32() as i32,
                                },
                            );
                        }
                        _ => {}
                    }
                }
            }
            let final_time = (time * ticks_per_second as f64) as i32;
            let serialized = trees.seal(final_time);

            let keys: Vec<_> = serialized
                .into_iter()
                .map(|s| CakeBlock {
                    start_time: 0,
                    end_time: final_time as u32,
                    tree: s,
                })
                .collect();

            (keys, note_count)
        });

        let audio_join_handle = thread::spawn(|| {
            let vec: Vec<_> = CompressedAudio::build_blocks(audio_rcv.into_iter()).collect();
            vec
        });

        let mut length = 0.0;

        // Write events to the threads
        for batch in merged {
            length += batch.delta;
            let batch = Arc::new(batch);
            key_snd.send(batch.clone()).unwrap();
            audio_snd.send(batch).unwrap();
        }
        // Drop the writers so the threads finish
        drop(key_snd);
        drop(audio_snd);

        let (keys, note_count) = key_join_handle.join().unwrap();
        let audio = audio_join_handle.join().unwrap();

        let mut timer = TimeKeeper::new(settings.start_delay);

        InRamAudioPlayer::new(audio, timer.get_listener(), player).spawn_playback();

        Ok(CakeMIDIFile {
            blocks: keys,
            timer,
            length,
            note_count,
            ticks_per_second,
            signature,
        })
    }

    pub fn key_blocks(&self) -> &[CakeBlock] {
        &self.blocks
    }

    pub fn ticks_per_second(&self) -> u32 {
        self.ticks_per_second
    }

    pub fn current_time(&self) -> Duration {
        self.timer.get_time()
    }

    pub fn cake_signature(&self) -> CakeSignature {
        CakeSignature {
            file_signature: self.signature.clone(),
            note_count: self.note_count,
            buffer_sizes: self.blocks.iter().map(|b| b.tree.len()).collect(),
        }
    }
}

/// A struct that uniquely identifies a cake midi file.
/// This lets the renderer know if the file has changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CakeSignature {
    file_signature: MIDIFileUniqueSignature,
    note_count: u64,
    buffer_sizes: Vec<usize>,
}

impl MIDIFileBase for CakeMIDIFile {
    fn midi_length(&self) -> Option<f64> {
        Some(self.length)
    }

    fn parsed_up_to(&self) -> Option<f64> {
        None
    }

    fn timer(&self) -> &TimeKeeper {
        &self.timer
    }

    fn timer_mut(&mut self) -> &mut TimeKeeper {
        &mut self.timer
    }

    fn allows_seeking_backward(&self) -> bool {
        true
    }

    fn stats(&self) -> MIDIFileStats {
        let time = self.timer.get_time().as_seconds_f64();
        let time_int = (time * self.ticks_per_second as f64) as i32;

        let passed_notes = self
            .key_blocks()
            .iter()
            .map(|b| b.get_notes_passed_at(time_int) as u64)
            .sum();

        MIDIFileStats {
            total_notes: Some(self.note_count),
            passed_notes: Some(passed_notes),
        }
    }

    fn signature(&self) -> &MIDIFileUniqueSignature {
        &self.signature
    }
}
