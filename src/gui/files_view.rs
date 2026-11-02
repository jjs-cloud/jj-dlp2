//! View of the Files tab.

use super::files::{FilesMsg, Panel, PanelKind, ROW_H, scroll_id, trim_id};
use super::skin::{self, BtnKind, Skin};
use super::view::{btn_label, muted, t, warn_text};
use super::{App, Msg};
use crate::filemgr::rows::{Row as FRow, Sort, SortKey};
use crate::filemgr::{file_name, fmt_mtime, human_rate, human_size};
use iced::border::Radius;
use iced::widget::text::Wrapping;
use iced::widget::{Column, Space, button, column, container, mouse_area, row, scrollable, text_input, toggler};
use iced::{Alignment, Background, Border, Element, Length, Theme};
use std::time::Instant;

const W_STATUS: f32 = 90.0;
const W_MODIFIED: f32 = 160.0;
const W_SIZE: f32 = 90.0;
const W_RATE: f32 = 100.0;
const INDENT: f32 = 18.0;

fn act<'a>(s: Skin, label: &'static str, m: Option<FilesMsg>, kind: BtnKind) -> Element<'a, Msg> {
    button(btn_label(&s, label).size(13))
        .on_press_maybe(m.map(Msg::Files))
        .padding([6.0, 12.0])
        .style(skin::btn(s, kind))
        .into()
}

fn panel_kind(p: &Panel) -> PanelKind {
    match p {
        Panel::Fixup { .. } => PanelKind::Fixup,
        Panel::Trim { .. } => PanelKind::Trim,
        Panel::Move { .. } => PanelKind::Move,
    }
}

fn flag<'a>(label: &'static str, on: bool, make: fn(bool) -> FilesMsg) -> Element<'a, Msg> {
    toggler(on).label(label).on_toggle(move |v| Msg::Files(make(v))).text_size(12).into()
}

/// Three fixed-width HH : MM : SS fields.
fn time_fields<'a>(s: Skin, label: &'static str, is_end: bool, vals: &'a [String; 3]) -> Element<'a, Msg> {
    let base = usize::from(is_end) * 3;
    let field = |i: usize| {
        text_input("00", &vals[i])
            .id(trim_id(base + i))
            .on_input(move |v| Msg::Files(FilesMsg::TrimField(is_end, i, v)))
            .width(46.0)
            .padding([5.0, 6.0])
            .align_x(iced::alignment::Horizontal::Center)
            .style(skin::input(s))
    };
    row![
        container(t(&s, label).size(13)).width(48.0),
        field(0),
        t(&s, ":"),
        field(1),
        t(&s, ":"),
        field(2),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

fn row_style(s: Skin, selected: bool, shaded: bool) -> impl Fn(&Theme) -> container::Style {
    let base = skin::stripe_tinted(s, shaded, None);
    move |th| {
        if selected {
            container::Style {
                background: Some(Background::Color(skin::with_alpha(s.accent, 0.25))),
                border: Border { color: skin::with_alpha(s.accent, 0.6), width: 1.0, radius: Radius::from(4.0) },
                ..Default::default()
            }
        } else {
            base(th)
        }
    }
}

fn cell<'a>(w: f32, content: impl Into<Element<'a, Msg>>) -> Element<'a, Msg> {
    container(content).width(w).padding([0.0, 6.0]).into()
}

fn head_cell<'a>(s: Skin, sort: Sort, label: &'static str, key: SortKey, w: Length) -> Element<'a, Msg> {
    let arrow = if sort.key != key {
        ""
    } else if sort.asc {
        " ▲"
    } else {
        " ▼"
    };
    let kind = if sort.key == key { BtnKind::Primary } else { BtnKind::Ghost };
    button(btn_label(&s, format!("{label}{arrow}")).size(12).width(Length::Fill))
        .on_press(Msg::Files(FilesMsg::HeaderSort(key)))
        .padding([4.0, 6.0])
        .width(w)
        .style(skin::btn(s, kind))
        .into()
}

impl App {
    pub(super) fn files_view(&self, s: Skin) -> Element<'_, Msg> {
        let f = &self.files;
        let has_file = f.file_selected();
        let open = f.panel.as_ref().map(panel_kind);
        let kind_for = |k: PanelKind| if open == Some(k) { BtnKind::Primary } else { BtnKind::Ghost };
        let panel_btn = |label: &'static str, k: PanelKind| {
            act(s, label, has_file.then_some(FilesMsg::OpenPanel(k)), kind_for(k))
        };

        let title = row![
            self.section_title(s, "FILES"),
            Space::new().width(Length::Fill),
            flag("Group by folder", f.group, FilesMsg::ToggleGroup),
            flag("Delete empty folders", f.delete_empty, FilesMsg::ToggleDeleteEmpty),
        ]
        .spacing(16)
        .align_y(Alignment::Center);

        let all_label = if f.any_collapsed() { "Expand all" } else { "Collapse all" };
        let actions = row![
            act(s, all_label, f.has_subfolders().then_some(FilesMsg::ToggleAll), BtnKind::Ghost),
            Space::new().width(Length::Fill),
            act(s, "Open", has_file.then_some(FilesMsg::OpenSelected), BtnKind::Ghost),
            act(s, "Show in folder", f.sel.is_some().then_some(FilesMsg::RevealSelected), BtnKind::Ghost),
            panel_btn("Fixup", PanelKind::Fixup),
            panel_btn("Trim", PanelKind::Trim),
            panel_btn("Move", PanelKind::Move),
            act(s, "Delete", has_file.then_some(FilesMsg::DeleteSelected), BtnKind::Danger),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mut top = Column::new().spacing(8);
        if let Some(p) = &f.panel {
            top = top.push(self.files_panel(s, p));
        }
        for j in &f.jobs {
            let finishing = j.token.finishing();
            top = top.push(
                row![
                    t(&s, format!("{}: {}", j.kind.label(), file_name(&j.target))).size(13),
                    Space::new().width(Length::Fill),
                    act(s, if finishing { "Finishing…" } else { "Cancel" }, (!finishing).then_some(FilesMsg::CancelJob(j.id)), BtnKind::Danger),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            );
        }
        if let Some(msg) = f.status_text(self.now) {
            top = top.push(muted(&s, msg));
        }

        let head = row![
            Space::new().width(INDENT + 4.0),
            head_cell(s, f.sort, "Name", SortKey::Name, Length::Fill),
            container(muted(&s, "Status")).width(W_STATUS).padding([0.0, 6.0]),
            head_cell(s, f.sort, "Modified", SortKey::Modified, Length::Fixed(W_MODIFIED)),
            head_cell(s, f.sort, "Size", SortKey::Size, Length::Fixed(W_SIZE)),
            head_cell(s, f.sort, "Rate", SortKey::Rate, Length::Fixed(W_RATE)),
            Space::new().width(8.0),
        ]
        .spacing(0)
        .align_y(Alignment::Center);

        let mut list = Column::new().padding(iced::Padding { right: 8.0, ..Default::default() });
        if self.settings.sites.is_empty() {
            list = list.push(container(muted(&s, "No sites yet. Add one in Settings to see its recordings here.")).padding(16));
        }
        let now = Instant::now();
        for (i, r) in f.rows.iter().enumerate() {
            list = list.push(self.files_row(s, i, r, now));
        }
        if let Some(m) = &f.moving {
            list = list.push(container(muted(&s, "MOVING")).center_y(ROW_H).padding([0.0, 6.0]));
            let line = row![
                Space::new().width(INDENT + 4.0),
                container(t(&s, m.name.as_str()).size(13).wrapping(Wrapping::None)).width(Length::Fill).clip(true),
                cell(W_STATUS, t(&s, "WRITING").size(12).color(s.live)),
                Space::new().width(W_MODIFIED + 6.0),
                cell(W_SIZE, muted(&s, human_size(m.size))),
                cell(W_RATE, muted(&s, human_rate(m.rate))),
            ]
            .align_y(Alignment::Center);
            list = list.push(container(line).center_y(ROW_H).padding([0.0, 6.0]).width(Length::Fill).style(skin::stripe_tinted(s, false, None)));
        }

        let total: u64 = f.records.values().map(|r| r.size).sum();
        let footer = row![
            muted(&s, format!("{} files · {}", f.records.len(), human_size(total))),
            Space::new().width(Length::Fill),
            muted(&s, "Enter / double-click: open    Del: recycle bin    Shift+Del: delete permanently").wrapping(Wrapping::None),
        ];

        let body = column![
            title,
            actions,
            top,
            head,
            scrollable(list).id(scroll_id()).on_scroll(|v| Msg::Files(FilesMsg::Scrolled(v))).height(Length::Fill),
            footer,
        ]
        .spacing(10);
        container(body).padding(14).width(Length::Fill).height(Length::Fill).style(skin::card(s, None)).into()
    }

    fn files_row<'a>(&'a self, s: Skin, i: usize, r: &'a FRow, now: Instant) -> Element<'a, Msg> {
        let f = &self.files;
        let Some(sel) = r.sel() else {
            return container(muted(&s, "No files yet")).center_y(ROW_H).padding([0.0, 24.0]).into();
        };
        let selected = f.sel.as_ref() == Some(&sel);
        let line: Element<'a, Msg> = match r {
            FRow::Folder { path, name, count, collapsed, depth } => row![
                Space::new().width(*depth as f32 * INDENT),
                button(t(&s, if *collapsed { "▶" } else { "▼" }).size(12).color(s.accent2))
                    .on_press(Msg::Files(FilesMsg::ToggleFolder(path.clone())))
                    .padding([0.0, 4.0])
                    .style(button::text),
                container(t(&s, name.as_str()).size(13).color(s.accent2).wrapping(Wrapping::None)).width(Length::Fill).clip(true),
                muted(&s, format!("{count} files")),
            ]
            .spacing(2)
            .align_y(Alignment::Center)
            .into(),
            FRow::File { path, prefix, name, depth } => {
                let job = f.job_for(path);
                let (status, rate): (Element<'a, Msg>, String) = match (job, f.records.get(path)) {
                    (Some(j), _) => (warn_text(&s, j.kind.busy()).into(), "—".to_string()),
                    (None, Some(rec)) if rec.writing(now) => (t(&s, "WRITING").size(12).color(s.live).into(), human_rate(rec.rate)),
                    _ => (muted(&s, "IDLE").into(), "—".to_string()),
                };
                let (modified, size) = f.records.get(path).map_or((String::new(), String::new()), |rec| (fmt_mtime(rec.mtime), human_size(rec.size)));
                row![
                    Space::new().width(*depth as f32 * INDENT + INDENT + 4.0),
                    container(
                        row![
                            muted(&s, prefix.as_str()).wrapping(Wrapping::None),
                            t(&s, name.as_str()).size(13).wrapping(Wrapping::None),
                        ]
                        .align_y(Alignment::Center),
                    )
                    .width(Length::Fill)
                    .clip(true),
                    cell(W_STATUS, status),
                    cell(W_MODIFIED, muted(&s, modified)),
                    cell(W_SIZE, muted(&s, size)),
                    cell(W_RATE, muted(&s, rate)),
                ]
                .align_y(Alignment::Center)
                .into()
            }
            FRow::Empty => return Space::new().into(),
        };
        let boxed = container(line).center_y(ROW_H).padding([0.0, 6.0]).width(Length::Fill).style(row_style(s, selected, i % 2 == 1));
        mouse_area(boxed)
            .on_press(Msg::Files(FilesMsg::Select(sel.clone())))
            .on_double_click(Msg::Files(FilesMsg::Activate(sel)))
            .into()
    }

    fn files_panel<'a>(&'a self, s: Skin, p: &'a Panel) -> Element<'a, Msg> {
        let close = || act(s, "Cancel", Some(FilesMsg::ClosePanel), BtnKind::Ghost);
        let content: Element<'a, Msg> = match p {
            Panel::Fixup { target, delete_original, convert_mp4 } => column![
                t(&s, format!("Fixup: {}", file_name(target))).size(13),
                flag("Delete original when finished", *delete_original, FilesMsg::FixupDelete),
                flag("Convert to MP4 (no re-encode)", *convert_mp4, FilesMsg::FixupMp4),
                row![act(s, "Run fixup", Some(FilesMsg::RunFixup), BtnKind::Primary), close()].spacing(8),
            ]
            .spacing(8)
            .into(),
            Panel::Trim { target, start, end, delete_original, convert_mp4 } => column![
                t(&s, format!("Trim: {}", file_name(target))).size(13),
                time_fields(s, "Start", false, start),
                time_fields(s, "End", true, end),
                flag("Delete original when finished", *delete_original, FilesMsg::TrimDelete),
                flag("Convert to MP4 (no re-encode)", *convert_mp4, FilesMsg::TrimMp4),
                row![act(s, "Run trim", Some(FilesMsg::RunTrim), BtnKind::Primary), close()].spacing(8),
            ]
            .spacing(8)
            .into(),
            Panel::Move { target, fixup } => column![
                t(&s, format!("Move: {}", file_name(target))).size(13),
                flag("Fixup the file during the move", *fixup, FilesMsg::MoveFixup),
                muted(&s, "Pick the destination in the save dialog; you can rename the file there too."),
                row![act(s, "Choose destination…", Some(FilesMsg::ChooseMoveDest), BtnKind::Primary), close()].spacing(8),
            ]
            .spacing(8)
            .into(),
        };
        container(content).padding(12).width(Length::Fill).style(skin::inset(s)).into()
    }
}

