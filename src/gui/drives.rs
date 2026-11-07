//! Disk-space display: the header chips, the Settings > General card and the background refresh.

use super::skin::{self, BtnKind, Skin};
use super::view::{btn_label, card_box, chip, muted, t};
use super::{prefs, App, Msg};
use crate::policy::disks::{self, DiskInfo, Snapshot};
use crate::policy::layout;
use iced::widget::{button, column, container, pick_list, row, tooltip, Row};
use iced::{Alignment, Element, Length, Task};
use std::path::{Path, PathBuf};

/// Seconds between free-space refreshes.
const REFRESH_SECS: u64 = 30;
/// Drive chips shown in the header before the rest collapse into one.
const HEADER_MAX: usize = 3;

/// A drive offered in the "Add a drive" list.
#[derive(Debug, Clone, PartialEq)]
pub struct DriveChoice {
    mount: PathBuf,
    text: String,
}

impl std::fmt::Display for DriveChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

/// The drive's name for a chip: the last part of the mount path, else the whole label.
fn short_label(d: &DiskInfo) -> String {
    d.mount.file_name().map_or_else(|| d.label(), |n| n.to_string_lossy().into_owned())
}

/// A tooltip bubble holding `text`.
fn tip<'a>(s: Skin, text: String) -> Element<'a, Msg> {
    container(iced::widget::text(text).font(s.font).size(12).color(s.text))
        .padding([4.0, 8.0])
        .style(skin::card(s, None))
        .into()
}

impl App {
    /// The output folders in use, each labelled by what uses it.
    fn disk_folders(&self) -> Vec<(String, PathBuf)> {
        let global = &self.settings.default_output_dir;
        let mut folders = vec![("Global".to_string(), PathBuf::from(layout::base_dir("", global)))];
        for site in &self.settings.sites {
            folders.push((site.name.clone(), PathBuf::from(layout::base_dir(&site.output_dir, global))));
        }
        for site in &self.posts.cfg.sites {
            folders.push((format!("Posts: {}", site.name), site.output_folder(global)));
        }
        folders
    }

    /// Starts a background refresh when the folders changed or the numbers are old.
    pub(super) fn disks_tick(&mut self) -> Task<Msg> {
        let stale = self.now.saturating_sub(self.disks_at) >= REFRESH_SECS;
        if self.disks_busy || !(self.disks_dirty || stale) {
            return Task::none();
        }
        self.disks_busy = true;
        self.disks_dirty = false;
        self.disks_at = self.now;
        let (folders, extras) = (self.disk_folders(), self.extra_drives.clone());
        Task::perform(
            async move { tokio::task::spawn_blocking(move || disks::snapshot(&folders, &extras)).await.unwrap_or_default() },
            Msg::DisksRefreshed,
        )
    }

    pub(super) fn on_disks(&mut self, snap: Snapshot) {
        self.disks = snap;
        self.disks_busy = false;
    }

    pub(super) fn add_drive(&mut self, mount: PathBuf) {
        if !self.extra_drives.contains(&mount) {
            self.extra_drives.push(mount);
            self.save_drives();
        }
    }

    pub(super) fn remove_drive(&mut self, mount: &Path) {
        self.extra_drives.retain(|m| m.as_path() != mount);
        self.save_drives();
    }

    fn save_drives(&mut self) {
        let list: Vec<String> = self.extra_drives.iter().map(|m| m.to_string_lossy().into_owned()).collect();
        prefs::save_extra_drives(&list);
        self.disks_dirty = true;
    }

    /// One header chip per connected drive, low ones first; `None` when there are none.
    pub(super) fn disk_chips(&self, s: Skin) -> Option<Element<'_, Msg>> {
        let mut shown: Vec<&DiskInfo> = self.disks.disks.iter().filter(|d| d.present).collect();
        if shown.is_empty() {
            return None;
        }
        shown.sort_by_key(|d| !d.is_low());
        let mut chips = Row::new().spacing(6).align_y(Alignment::Center);
        for d in shown.iter().take(HEADER_MAX) {
            chips = chips.push(self.disk_chip(s, d));
        }
        if shown.len() > HEADER_MAX {
            let rest = &shown[HEADER_MAX..];
            let color = if rest.iter().any(|d| d.is_low()) { s.warn } else { s.muted };
            let lines: Vec<String> = rest.iter().map(|d| format!("{}: {} free", d.label(), disks::format_size(d.free))).collect();
            chips = chips.push(tooltip(chip(s, format!("+{}", rest.len()), color), tip(s, lines.join("\n")), tooltip::Position::Bottom));
        }
        Some(chips.into())
    }

    fn disk_chip(&self, s: Skin, d: &DiskInfo) -> Element<'_, Msg> {
        let free = disks::format_size(d.free);
        let color = if d.is_low() { s.warn } else { s.muted };
        let mut lines = vec![d.label(), format!("{free} free of {} ({:.0}%)", disks::format_size(d.total), d.free_percent())];
        if d.is_low() {
            lines.push("Low on space".to_string());
        }
        if !d.used_by.is_empty() {
            lines.push(format!("Used by: {}", d.used_by.join(", ")));
        }
        tooltip(chip(s, format!("{} {free} free", short_label(d)), color), tip(s, lines.join("\n")), tooltip::Position::Bottom).into()
    }

    /// The Settings > General card listing the tracked drives and the list for adding more.
    pub(super) fn drives_card(&self, s: Skin) -> Element<'_, Msg> {
        let mut col = column![
            self.section_title(s, "DISK SPACE"),
            muted(&s, "Drives holding your output folders are tracked automatically and shown in the header and in notifications."),
            muted(&s, format!("Marked low below {}% or {} free.", disks::LOW_PERCENT, disks::format_size(disks::LOW_BYTES))),
        ]
        .spacing(8);
        if self.disks.disks.is_empty() {
            col = col.push(muted(&s, "No drives found for the output folders."));
        }
        for d in &self.disks.disks {
            col = col.push(self.drive_row(s, d));
        }
        let choices: Vec<DriveChoice> = self
            .disks
            .others
            .iter()
            .map(|d| DriveChoice { mount: d.mount.clone(), text: format!("{}  ({} free)", d.label(), disks::format_size(d.free)) })
            .collect();
        if !choices.is_empty() {
            col = col.push(
                pick_list(choices, None::<DriveChoice>, |c: DriveChoice| Msg::AddDrive(c.mount))
                    .placeholder("Add a drive...")
                    .padding(8),
            );
        }
        card_box(s, col)
    }

    fn drive_row(&self, s: Skin, d: &DiskInfo) -> Element<'_, Msg> {
        let detail = if d.present {
            format!("{} free of {}", disks::format_size(d.free), disks::format_size(d.total))
        } else {
            "Not connected".to_string()
        };
        let color = if d.is_low() { s.warn } else { s.text };
        let who = if d.used_by.is_empty() { "Added manually".to_string() } else { format!("Used by {}", d.used_by.join(", ")) };
        let mut r = row![
            t(&s, d.label()).size(13).width(Length::Fixed(160.0)),
            iced::widget::text(detail).font(s.font).size(13).color(color).width(Length::Fixed(190.0)),
            muted(&s, who).width(Length::Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        if d.extra {
            r = r.push(
                button(btn_label(&s, "Remove").size(12))
                    .on_press(Msg::RemoveDrive(d.mount.clone()))
                    .padding([4.0, 10.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
            );
        }
        r.into()
    }
}
