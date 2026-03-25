use dioxus::prelude::*;

/// Provided by the mobile `PlatformMenu` variant so that `PlatformMenuItem` components
/// rendered in its content subtree can close the sheet after an action. Not needed on
/// desktop — `DropdownMenuItem` closes the menu via its own primitives context.
#[derive(Clone, Copy)]
pub struct PlatformMenuCtx {
    open: Signal<bool>,
}

impl PlatformMenuCtx {
    fn close(&mut self) {
        self.open.set(false);
    }
}

/// Platform-adaptive menu container. On desktop/web renders as a `DropdownMenu`
/// (keyboard navigation, ARIA roles, hover styles). On mobile renders as a bottom sheet.
/// Content items should be `PlatformMenuItem` to get platform-correct close behavior.
#[cfg(not(feature = "mobile"))]
#[component]
pub fn PlatformMenu(trigger: Element, content: Element) -> Element {
    use crate::components::dropdown_menu::{
        DropdownMenu, DropdownMenuContent, DropdownMenuTrigger,
    };
    rsx! {
        DropdownMenu {
            DropdownMenuTrigger { {trigger} }
            DropdownMenuContent { {content} }
        }
    }
}

#[cfg(feature = "mobile")]
#[component]
pub fn PlatformMenu(trigger: Element, content: Element) -> Element {
    use crate::components::PlatformPopover;
    let mut open = use_signal(|| false);
    use_context_provider(|| PlatformMenuCtx { open });
    rsx! {
        PlatformPopover {
            open: open(),
            on_open_change: move |v| open.set(v),
            trigger,
            content,
        }
    }
}

/// Platform-adaptive menu item. On desktop/web wraps `DropdownMenuItem` (keyboard
/// navigation, auto-close via primitives context). On mobile renders as a styled div
/// and closes the sheet via `PlatformMenuCtx` after calling `on_select`.
///
/// `on_select` receives the typed `value` prop, matching the `DropdownMenuItem` API.
#[cfg(not(feature = "mobile"))]
#[component]
pub fn PlatformMenuItem<T: Clone + PartialEq + 'static>(
    value: T,
    index: usize,
    on_select: Callback<T>,
    #[props(default = false)] disabled: bool,
    children: Element,
) -> Element {
    use crate::components::dropdown_menu::DropdownMenuItem;
    rsx! {
        DropdownMenuItem::<T> {
            value,
            index,
            on_select,
            disabled,
            {children}
        }
    }
}

#[cfg(feature = "mobile")]
#[component]
pub fn PlatformMenuItem<T: Clone + PartialEq + 'static>(
    value: T,
    index: usize,
    on_select: Callback<T>,
    #[props(default = false)] disabled: bool,
    children: Element,
) -> Element {
    let _ = index; // unused on mobile; present for API uniformity with desktop
    let mut ctx = use_context::<PlatformMenuCtx>();
    rsx! {
        div {
            class: "flex dropdown-menu-item justify-center w-full items-center",
            onclick: move |_| {
                if !disabled {
                    on_select.call(value.clone());
                    ctx.close();
                }
            },
            {children}
        }
    }
}
