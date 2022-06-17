pub struct InRamNoteBlock {
    pub start: f64,
    pub max_length: f32,
    pub notes: Vec<BasicMIDINote>,
}

#[derive(Debug, Clone)]
pub struct BasicMIDINote {
    pub len: f32,
    pub track_chan: u32,
}

impl InRamNoteBlock {
    /// Creates a new block from an iterator of Track/Channel values.
    /// This assumes that the lengths will be added in the future.
    pub fn new_from_trackchans(
        time: f64,
        track_chans_iter: impl ExactSizeIterator<Item = u32>,
    ) -> Self {
        let mut notes: Vec<BasicMIDINote> = Vec::with_capacity(track_chans_iter.len());

        for track_chan in track_chans_iter {
            notes.push(BasicMIDINote {
                len: 0.0,
                track_chan,
            });
        }

        InRamNoteBlock {
            start: time,
            notes,
            max_length: 0.0,
        }
    }

    pub fn set_note_end_time(&mut self, note_index: usize, end_time: f64) {
        let note = &mut self.notes[note_index];
        note.len = (end_time - self.start) as f32;
        self.max_length = self.max_length.max(note.len);
    }
}
