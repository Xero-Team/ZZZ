#[derive(Clone, Copy, PartialEq, Default)]
pub enum RowRenderMechanism {
    /// More correct for multiline content, but slower.
    #[default]
    VariableList,
    /// Default behaviour for now while resizable columns are being stabilized.
    #[allow(
        dead_code,
        reason = "Exposed in dev-tools and kept for fallback rendering mode"
    )]
    UniformList,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DelimiterSelection {
    #[default]
    Auto,
    Character(char),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum HeaderMode {
    #[default]
    FirstRow,
    NoHeader,
}

#[derive(Default, Clone, Copy)]
pub enum VerticalAlignment {
    /// Align text to the top of cells
    #[default]
    Top,
    /// Center text vertically in cells
    Center,
}

#[derive(Default, Clone, Copy)]
pub enum RowIdentifiers {
    /// Show logical source row numbers, counting multiline records as a single row.
    #[default]
    SrcLines,
    /// Show sequential row numbers starting from 1
    RowNum,
}

/// How a column's filter values are ordered in its filter menu.
#[derive(Default, Clone, Copy, PartialEq)]
pub enum FilterSortOrder {
    /// Sort alphabetically (A→Z), then by number of occurrences descending within ties
    #[default]
    AlphaThenCount,
    /// Sort by number of occurrences descending, then alphabetically within ties
    CountThenAlpha,
}

#[derive(Clone)]
pub(crate) struct CsvPreviewSettings {
    pub(crate) delimiter: DelimiterSelection,
    pub(crate) header_mode: HeaderMode,
    pub(crate) rendering_with: RowRenderMechanism,
    pub(crate) vertical_alignment: VerticalAlignment,
    pub(crate) numbering_type: RowIdentifiers,
    pub(crate) filter_sort_order: FilterSortOrder,
    pub(crate) show_debug_info: bool,
    #[cfg(feature = "dev-tools")]
    pub(crate) show_perf_metrics_overlay: bool,
    pub(crate) multiline_cells_enabled: bool,
}

impl Default for CsvPreviewSettings {
    fn default() -> Self {
        Self {
            delimiter: DelimiterSelection::Auto,
            header_mode: HeaderMode::FirstRow,
            rendering_with: RowRenderMechanism::VariableList,
            vertical_alignment: VerticalAlignment::Top,
            numbering_type: RowIdentifiers::SrcLines,
            filter_sort_order: FilterSortOrder::default(),
            show_debug_info: false,
            #[cfg(feature = "dev-tools")]
            show_perf_metrics_overlay: false,
            multiline_cells_enabled: true,
        }
    }
}

impl CsvPreviewSettings {
    /// `multiline_cells_enabled` only makes sense with `VariableList`, which
    /// supports per-row heights; `UniformList` requires every row to share one
    /// height, so multiline is never honored there regardless of the setting.
    pub(crate) fn multiline_cells_effectively_enabled(&self) -> bool {
        self.multiline_cells_enabled && self.rendering_with == RowRenderMechanism::VariableList
    }
}
