//! The components module contains all shared components for our app. Components are the building blocks of dioxus apps.
//! They can be used to defined common UI elements like buttons, forms, and modals. In this template, we define a Hero
//! component  to be used in our app.

mod command_palette;
pub use command_palette::{Command, CommandPalette};

mod entry_view;
pub use entry_view::EntryView;

pub mod attribute_view;
pub use attribute_view::{AttributeRow, AttributeView, TemporalAttribute};

mod log_date_picker;
pub use log_date_picker::LogDatePicker;

pub mod calendar;
pub mod context_menu;
pub mod popover;
pub mod dropdown_menu;

mod frequency_heatmap;
pub use frequency_heatmap::{FrequencyGrid, FrequencyHeatmap, Quantizer};
pub mod input;
pub mod button;
pub mod sheet;

mod platform_popover;
pub use platform_popover::PlatformPopover;

mod platform_menu;
pub use platform_menu::{PlatformMenu, PlatformMenuItem};
