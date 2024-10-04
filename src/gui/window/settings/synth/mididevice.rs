use std::sync::Arc;

use egui_extras::{Column, TableBuilder};

use crate::{
    audio_playback::WasabiAudioPlayer,
    gui::window::{GuiWasabiWindow, LoadingStatus},
    settings::WasabiSettings,
};

use super::SettingsWindow;

impl SettingsWindow {
    pub fn show_mididevice_settings(
        &mut self,
        ui: &mut egui::Ui,
        settings: &mut WasabiSettings,
        width: f32,
        synth: Arc<WasabiAudioPlayer>,
        loading_status: Arc<LoadingStatus>,
    ) {
        egui::Frame::default()
            .rounding(egui::Rounding::same(8.0))
            .stroke(ui.style().visuals.widgets.noninteractive.bg_stroke)
            .show(ui, |ui| {
                TableBuilder::new(ui)
                    .striped(true)
                    .cell_layout(egui::Layout::centered_and_justified(
                        egui::Direction::LeftToRight,
                    ))
                    .resizable(true)
                    .column(Column::exact(width).resizable(false))
                    .body(|mut body| {
                        let row_height = super::super::SPACING[1] * 3.0;

                        let mut temp = self.midi_devices.clone();
                        for i in temp.iter_mut() {
                            i.selected = false;
                        }
                        let mut changed = false;
                        for (i, device) in self.midi_devices.iter_mut().enumerate() {
                            body.row(row_height, |mut row| {
                                row.col(|ui| {
                                    if ui
                                        .selectable_label(device.selected, device.name.clone())
                                        .clicked()
                                    {
                                        temp[i].selected = true;
                                        settings.synth.midi_device = device.name.clone();
                                        changed = true;
                                    }
                                });
                            });
                        }
                        if changed {
                            self.midi_devices = temp;
                            synth.switch(GuiWasabiWindow::create_synth(settings, loading_status));
                        }
                    });
            });
        ui.add_space(4.0);
        if ui.button("Refresh List").clicked() {
            self.load_midi_devices(settings);
        }
    }
}
