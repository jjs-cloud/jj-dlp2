//! Disk-space display: the header chips, the Settings > General card and the background refresh.

use super::skin::{self, BtnKind, Skin};
use super::view::{btn_label, card_box, chip, muted, t};
use super::{prefs, App, Msg};
use crate::policy::disks::{self, DiskInfo, Snapshot};
use crate::policy::layout;
use iced::widget::{button, column, container, pick_list, row, text_input, toggler, tooltip, Row};
use iced::{Alignment, Element, Length, Task};
use std::path::{Path, PathBuf};

/// Seconds between free-space refreshes.
const REFRESH_SECS: u64 = 30;
/// Drive chips shown in the header before the rest collapse into one.
const HEADER_MAX: usize = 6;

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
        self.apply_drive_order();
    }

    /// Sorts the tracked drives by the saved order; drives not in it keep their place at the end.
    fn apply_drive_order(&mut self) {
        let order = &self.drive_order;
        self.disks.disks.sort_by_key(|d| order.iter().position(|m| *m == d.mount).unwrap_or(usize::MAX));
    }

    /// Moves a tracked drive `delta` places in the list and saves the order.
    pub(super) fn move_drive(&mut self, mount: &Path, delta: isize) {
        let mut order: Vec<PathBuf> = self.disks.disks.iter().map(|d| d.mount.clone()).collect();
        let Some(i) = order.iter().position(|m| m == mount) else { return };
        let j = (i as isize + delta).clamp(0, order.len() as isize - 1) as usize;
        let m = order.remove(i);
        order.insert(j, m);
        prefs::save_drive_order(&order.iter().map(|m| m.to_string_lossy().into_owned()).collect::<Vec<_>>());
        self.drive_order = order;
        self.apply_drive_order();
    }

    /// Applies and saves the disk-space display settings.
    pub(super) fn set_disk_limits(&mut self, limits: disks::Limits) {
        self.disk_limits = limits;
        disks::set_limits(limits);
        prefs::save_disk_limits(limits);
    }

    /// Takes the typed GB limit; blank falls back to the default.
    pub(super) fn edit_low_gb(&mut self, text: String) {
        let text: String = text.chars().filter(char::is_ascii_digit).take(6).collect();
        let low_gb = text.parse().unwrap_or(disks::DEFAULT_LOW_GB);
        self.low_gb_text = text;
        self.set_disk_limits(disks::Limits { low_gb, ..self.disk_limits });
    }

    /// Takes the typed percentage limit, at most 100; blank falls back to the default.
    pub(super) fn edit_low_percent(&mut self, text: String) {
        let mut text: String = text.chars().filter(char::is_ascii_digit).take(3).collect();
        if text.parse::<u64>().is_ok_and(|n| n > 100) {
            text = "100".to_string();
        }
        let low_percent = text.parse().unwrap_or(disks::DEFAULT_LOW_PERCENT);
        self.low_percent_text = text;
        self.set_disk_limits(disks::Limits { low_percent, ..self.disk_limits });
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

    /// One header chip per connected drive, in list order; `None` when there are none.
    pub(super) fn disk_chips(&self, s: Skin) -> Option<Element<'_, Msg>> {
        let shown: Vec<&DiskInfo> = self.disks.disks.iter().filter(|d| d.present).collect();
        if shown.is_empty() {
            return None;
        }
        let mut chips = Row::new().spacing(6).align_y(Alignment::Center);
        for d in shown.iter().take(HEADER_MAX) {
            chips = chips.push(self.disk_chip(s, d));
        }
        if shown.len() > HEADER_MAX {
            let rest = &shown[HEADER_MAX..];
            let color = if rest.iter().any(|d| d.is_low()) { s.warn } else { s.muted };
            let lines: Vec<String> = rest.iter().map(|d| format!("{}: {} free", d.label(), disks::free_text(d.total, d.free))).collect();
            chips = chips.push(tooltip(chip(s, format!("+{}", rest.len()), color), tip(s, lines.join("\n")), tooltip::Position::Bottom));
        }
        Some(chips.into())
    }

    fn disk_chip(&self, s: Skin, d: &DiskInfo) -> Element<'_, Msg> {
        let free = disks::free_text(d.total, d.free);
        let color = if d.is_low() { s.warn } else { s.muted };
        let mut lines = vec![d.label(), format!("{} free of {} ({:.0}%)", disks::format_size(d.free), disks::format_size(d.total), d.free_percent())];
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
            toggler(self.disk_limits.show_percent)
                .label("Show percentage")
                .on_toggle(Msg::DiskShowPercent)
                .text_size(12),
            self.low_limits_row(s),
        ]
        .spacing(8);
        if self.disks.disks.is_empty() {
            col = col.push(muted(&s, "No drives found for the output folders."));
        }
        let last = self.disks.disks.len().saturating_sub(1);
        for (i, d) in self.disks.disks.iter().enumerate() {
            col = col.push(self.drive_row(s, d, i, last));
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

    /// The inputs for the "Low" thresholds.
    fn low_limits_row(&self, s: Skin) -> Element<'_, Msg> {
        let pct_hint = disks::DEFAULT_LOW_PERCENT.to_string();
        let gb_hint = disks::DEFAULT_LOW_GB.to_string();
        row![
            muted(&s, "Mark a drive low below"),
            text_input(&pct_hint, &self.low_percent_text).on_input(Msg::DiskLowPercent).padding(5).size(13).width(Length::Fixed(70.0)).style(skin::input(s)),
            muted(&s, "% free or"),
            text_input(&gb_hint, &self.low_gb_text).on_input(Msg::DiskLowGb).padding(5).size(13).width(Length::Fixed(70.0)).style(skin::input(s)),
            muted(&s, "GB free"),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }

    fn drive_row(&self, s: Skin, d: &DiskInfo, i: usize, last: usize) -> Element<'_, Msg> {
        let detail = if d.present {
            format!("{} free of {} ({:.0}% free)", disks::format_size(d.free), disks::format_size(d.total), d.free_percent())
        } else {
            "Not connected".to_string()
        };
        let color = if d.is_low() { s.warn } else { s.text };
        let who = if d.used_by.is_empty() { "Added manually".to_string() } else { format!("Used by {}", d.used_by.join(", ")) };
        let mut r = row![
            t(&s, d.label()).size(13).width(Length::Fixed(160.0)),
            iced::widget::text(detail).font(s.font).size(13).color(color).width(Length::Fixed(260.0)),
            muted(&s, who).width(Length::Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let arrow = |label: &'static str, delta: isize, enabled: bool| {
            button(btn_label(&s, label).size(12))
                .on_press_maybe(enabled.then(|| Msg::MoveDrive(d.mount.clone(), delta)))
                .padding([4.0, 8.0])
                .style(skin::btn(s, BtnKind::Ghost))
        };
        if d.extra {
            r = r.push(
                button(btn_label(&s, "Remove").size(12))
                    .on_press(Msg::RemoveDrive(d.mount.clone()))
                    .padding([4.0, 10.0])
                    .style(skin::btn(s, BtnKind::Ghost)),
            );
        }
        r = r.push(arrow("\u{25B2}", -1, i > 0)).push(arrow("\u{25BC}", 1, i < last));
        r.into()
    }
}
