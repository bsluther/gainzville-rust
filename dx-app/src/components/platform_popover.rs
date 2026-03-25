use dioxus::prelude::*;
use dioxus_primitives::{ContentAlign, ContentSide};

#[cfg(not(feature = "mobile"))]
use crate::components::popover::{PopoverContent, PopoverRoot, PopoverTrigger};
#[cfg(feature = "mobile")]
use crate::components::sheet::{Sheet, SheetContent, SheetSide};

/// Platform-adaptive overlay: renders as a positioned popover on desktop/web and a
/// bottom sheet on mobile. The caller owns the `open` signal and passes it as a
/// controlled prop; content closures can capture and set it directly to close.
#[cfg(not(feature = "mobile"))]
#[component]
pub fn PlatformPopover(
    open: bool,
    on_open_change: EventHandler<bool>,
    trigger: Element,
    content: Element,
    #[props(default = ContentSide::Bottom)] side: ContentSide,
    #[props(default = ContentAlign::Start)] align: ContentAlign,
) -> Element {
    rsx! {
        PopoverRoot { open, on_open_change,
            PopoverTrigger { {trigger} }
            PopoverContent { side, align, {content} }
        }
    }
}

#[cfg(feature = "mobile")]
#[component]
pub fn PlatformPopover(
    open: bool,
    on_open_change: EventHandler<bool>,
    trigger: Element,
    content: Element,
    // These props are unused on mobile but present for API uniformity with the desktop variant.
    #[props(default = ContentSide::Bottom)] side: ContentSide,
    #[props(default = ContentAlign::Start)] align: ContentAlign,
) -> Element {
    let _ = (side, align);
    rsx! {
        div {
            class: "flex flex-1 items-center",
            onclick: move |_| on_open_change.call(true),
            {trigger}
        }
        Sheet { open, on_open_change,
            SheetContent {
                class: "w-full p-4 max-h-96 min-h-48 overflow-y-scroll",
                side: SheetSide::Bottom,
                {content}
            }
        }
    }
}
