//! 数据展示：`nsh_base::data_table` 的语义薄封装（行高 36、token 化
//! 分区样式），与 ListItem/StatusBar 同属数据区基础能力。

use nsh_base::{DataTableColumn, DataTableRows, DataTableStyles, data_table, theme};
use nsh_core::element::{Style, StyleRefinement};
use nsh_core::prelude::*;

fn part(f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Style {
    Style::from(f(StyleRefinement::default()))
}

/// 语义数据表：列定义 + 行数据，样式全部来自 token。
/// `PartStyle` 类型由 `nsh_base` 私有导出，经 `impl From<Style>` 构造。
pub fn table(columns: Vec<DataTableColumn>, rows: DataTableRows) -> AnyElement {
    data_table()
        .columns(columns)
        .data(rows)
        .row_height(px(36.0))
        .styles({
            let mut styles = DataTableStyles::default();
            styles.root = Some(
                part(|s| {
                    s.rounded(move || theme().radius)
                        .border(px(1.0))
                        .border_color(move || theme().border)
                        .bg(move || theme().card)
                })
                .into(),
            );
            styles.header = Some(
                part(|s| {
                    s.bg(move || theme().muted)
                        .text_color(move || theme().muted_foreground)
                })
                .into(),
            );
            styles.header_cell = Some(
                part(|s| {
                    s.font_weight(FontWeight::MEDIUM)
                        .text_color(move || theme().foreground)
                })
                .into(),
            );
            styles.row = Some(part(|s| s.text_color(move || theme().foreground)).into());
            styles.row_hover = Some(part(|s| s.bg(move || theme().accent.opacity(0.6))).into());
            styles.row_selected = Some(
                part(|s| {
                    s.bg(move || theme().accent)
                        .text_color(move || theme().accent_foreground)
                })
                .into(),
            );
            styles.cell = Some(part(|s| s.px(px(12.0))).into());
            styles.empty =
                Some(part(|s| s.text_color(move || theme().muted_foreground).p(px(24.0))).into());
            styles
        })
        .into_element()
}
