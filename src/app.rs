use crate::{
    github::{RELEASES_REPOSITORY_SLUG, RELEASES_REPOSITORY_URL},
    service::{AppService, DashboardSnapshot},
};
use eframe::egui::{self, Color32, Frame, RichText};
use std::sync::mpsc::{self, Receiver};

pub struct HeliumUpdaterApp {
    service: AppService,
    snapshot: DashboardSnapshot,
    pending_action: Option<Receiver<Result<DashboardSnapshot, String>>>,
}

impl HeliumUpdaterApp {
    pub fn new(creation_context: &eframe::CreationContext<'_>, service: AppService) -> Self {
        configure_theme(&creation_context.egui_ctx);

        let snapshot = service.initial_snapshot();
        let mut app = Self {
            service,
            snapshot,
            pending_action: None,
        };
        app.spawn_action(|service| service.startup_refresh());
        app
    }

    fn is_busy(&self) -> bool {
        self.pending_action.is_some()
    }

    fn poll_pending_action(&mut self) {
        let Some(receiver) = &self.pending_action else {
            return;
        };

        match receiver.try_recv() {
            Ok(Ok(snapshot)) => {
                self.snapshot = snapshot;
                self.pending_action = None;
            }
            Ok(Err(error)) => {
                let mut fallback = self.service.initial_snapshot();
                fallback.status_message = error;
                self.snapshot = fallback;
                self.pending_action = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pending_action = None;
            }
        }
    }

    fn spawn_action(
        &mut self,
        action: impl FnOnce(AppService) -> Result<DashboardSnapshot, String> + Send + 'static,
    ) {
        let service = self.service.clone();
        let (sender, receiver) = mpsc::channel();

        std::thread::spawn(move || {
            let _ = sender.send(action(service));
        });

        self.pending_action = Some(receiver);
    }
}

impl eframe::App for HeliumUpdaterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_pending_action();

        egui::CentralPanel::default()
            .frame(Frame::default().fill(Color32::from_rgb(244, 240, 230)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add_space(12.0);

                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new("Helium Browser Updater")
                                    .size(28.0)
                                    .strong()
                                    .color(Color32::from_rgb(44, 52, 64)),
                            );
                            ui.label(
                                RichText::new("Rust GUI for daily Helium installs and updates")
                                    .size(14.0)
                                    .color(Color32::from_rgb(91, 98, 113)),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "Release source: {RELEASES_REPOSITORY_SLUG}"
                                ))
                                    .size(13.0)
                                    .color(Color32::from_rgb(91, 98, 113)),
                            );
                            ui.label(
                                RichText::new(RELEASES_REPOSITORY_URL)
                                    .size(12.0)
                                    .color(Color32::from_rgb(91, 98, 113)),
                            );
                        });

                        ui.add_space(18.0);

                        let wide_layout = ui.available_width() >= 760.0;
                        let installed_detail = self
                            .snapshot
                            .installed_product_version
                            .as_ref()
                            .map(|value| format!("Chromium engine {value}"));
                        let latest_detail = self
                            .snapshot
                            .latest_product_version
                            .as_ref()
                            .map(|value| format!("Chromium engine {value}"));

                        if wide_layout {
                            ui.columns(2, |columns| {
                                info_card(
                                    &mut columns[0],
                                    "Installed version",
                                    self.snapshot.installed_label(),
                                    installed_detail.as_deref(),
                                );
                                info_card(
                                    &mut columns[1],
                                    "Latest release",
                                    self.snapshot.latest_label(),
                                    latest_detail.as_deref(),
                                );
                            });
                        } else {
                            info_card(
                                ui,
                                "Installed version",
                                self.snapshot.installed_label(),
                                installed_detail.as_deref(),
                            );
                            ui.add_space(12.0);
                            info_card(
                                ui,
                                "Latest release",
                                self.snapshot.latest_label(),
                                latest_detail.as_deref(),
                            );
                        }

                        ui.add_space(12.0);

                        full_width_group(ui, |ui| {
                            let mut automatic_updates = self.snapshot.automatic_updates_enabled;
                            let changed = ui
                                .add_enabled(
                                    !self.is_busy(),
                                    egui::Checkbox::new(
                                        &mut automatic_updates,
                                        "Automatic daily updates",
                                    ),
                                )
                                .changed();

                            ui.label(
                                RichText::new(format!(
                                    "Task Scheduler target: daily checks on {} systems. Task installed: {}.",
                                    self.snapshot.architecture_label,
                                    if self.snapshot.scheduled_task_present {
                                        "yes"
                                    } else {
                                        "no"
                                    }
                                ))
                                .size(13.0)
                                .color(Color32::from_rgb(91, 98, 113)),
                            );

                            if changed {
                                self.snapshot.automatic_updates_enabled = automatic_updates;
                                self.spawn_action(move |service| {
                                    service.set_automatic_updates(automatic_updates)
                                });
                            }
                        });

                        ui.add_space(12.0);

                        let check_button = egui::Button::new(
                            RichText::new("Check for updates")
                                .size(15.0)
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(52, 94, 83));

                        let install_label = if !self.snapshot.is_installed {
                            "Download and install Helium"
                        } else if self.snapshot.update_available == Some(true) {
                            "Download and install update"
                        } else {
                            "Install latest release"
                        };

                        let install_button = egui::Button::new(
                            RichText::new(install_label)
                                .size(15.0)
                                .color(Color32::from_rgb(44, 52, 64)),
                        )
                        .fill(Color32::from_rgb(236, 183, 85));

                        let delete_task_button = egui::Button::new(
                            RichText::new("Delete scheduled task")
                                .size(15.0)
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(150, 89, 76));

                        let install_enabled = !self.is_busy()
                            && (!self.snapshot.is_installed
                                || self.snapshot.update_available != Some(false));
                        let delete_task_enabled =
                            !self.is_busy() && self.snapshot.scheduled_task_present;

                        if wide_layout {
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(
                                        !self.is_busy(),
                                        check_button.min_size(egui::vec2(180.0, 42.0)),
                                    )
                                    .clicked()
                                {
                                    self.spawn_action(|service| service.check_for_updates());
                                }

                                if ui
                                    .add_enabled(
                                        install_enabled,
                                        install_button.min_size(egui::vec2(240.0, 42.0)),
                                    )
                                    .clicked()
                                {
                                    self.spawn_action(|service| service.install_or_update_now());
                                }

                                if ui
                                    .add_enabled(
                                        delete_task_enabled,
                                        delete_task_button.min_size(egui::vec2(210.0, 42.0)),
                                    )
                                    .clicked()
                                {
                                    self.spawn_action(|service| service.delete_scheduled_task());
                                }
                            });
                        } else {
                            let full_width = ui.available_width();

                            if ui
                                .add_enabled(
                                    !self.is_busy(),
                                    check_button.min_size(egui::vec2(full_width, 42.0)),
                                )
                                .clicked()
                            {
                                self.spawn_action(|service| service.check_for_updates());
                            }

                            if ui
                                .add_enabled(
                                    install_enabled,
                                    install_button.min_size(egui::vec2(full_width, 42.0)),
                                )
                                .clicked()
                            {
                                self.spawn_action(|service| service.install_or_update_now());
                            }

                            if ui
                                .add_enabled(
                                    delete_task_enabled,
                                    delete_task_button.min_size(egui::vec2(full_width, 42.0)),
                                )
                                .clicked()
                            {
                                self.spawn_action(|service| service.delete_scheduled_task());
                            }
                        }

                        ui.add_space(16.0);

                        full_width_group(ui, |ui| {
                            ui.label(
                                RichText::new(match self.snapshot.update_available {
                                    Some(true) if self.snapshot.is_installed => {
                                        "Update status: new release available"
                                    }
                                    Some(true) => "Update status: ready to install",
                                    Some(false) => "Update status: already up to date",
                                    None => "Update status: not checked yet",
                                })
                                .size(18.0)
                                .strong()
                                .color(Color32::from_rgb(44, 52, 64)),
                            );

                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(format!(
                                    "Last successful check: {}",
                                    self.snapshot.last_checked_label()
                                ))
                                .size(14.0)
                                .color(Color32::from_rgb(91, 98, 113)),
                            );

                            if let Some(published_at) = &self.snapshot.latest_release_published_at {
                                ui.label(
                                    RichText::new(format!(
                                        "Latest release published: {published_at}"
                                    ))
                                    .size(14.0)
                                    .color(Color32::from_rgb(91, 98, 113)),
                                );
                            }

                            ui.add_space(10.0);
                            ui.label(
                                RichText::new(&self.snapshot.status_message)
                                    .size(15.0)
                                    .color(Color32::from_rgb(60, 63, 70)),
                            );

                            if self.is_busy() {
                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(
                                        RichText::new("Working on it...")
                                            .size(14.0)
                                            .color(Color32::from_rgb(91, 98, 113)),
                                    );
                                });
                            }
                        });

                        ui.add_space(8.0);
                    });
            });

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

fn info_card(ui: &mut egui::Ui, label: &str, value: &str, detail: Option<&str>) {
    Frame::group(ui.style())
        .fill(Color32::from_rgb(248, 245, 237))
        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(210, 204, 190)))
        .show(ui, |ui| {
            ui.set_min_height(120.0);
            ui.add_space(8.0);
            ui.label(
                RichText::new(label)
                    .size(14.0)
                    .color(Color32::from_rgb(91, 98, 113)),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(value)
                    .size(26.0)
                    .strong()
                    .color(Color32::from_rgb(44, 52, 64)),
            );

            if let Some(detail) = detail {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(detail)
                        .size(13.0)
                        .color(Color32::from_rgb(91, 98, 113)),
                );
            }
        });
}

fn full_width_group(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    Frame::group(ui.style())
        .fill(Color32::from_rgb(248, 245, 237))
        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(210, 204, 190)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
}

fn configure_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.override_text_color = Some(Color32::from_rgb(44, 52, 64));
    visuals.widgets.active.bg_fill = Color32::from_rgb(52, 94, 83);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(68, 120, 106);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(235, 229, 214);
    visuals.window_fill = Color32::from_rgb(244, 240, 230);

    let mut style = (*ctx.style()).clone();
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(12.0, 12.0);
    style.spacing.button_padding = egui::vec2(14.0, 10.0);
    style.spacing.window_margin = egui::Margin::same(18);
    ctx.set_style(style);
}
