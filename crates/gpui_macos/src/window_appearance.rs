use crate::NSStringExt;
use gpui::WindowAppearance;
use objc2::{msg_send, rc::Retained, runtime::AnyObject};
use objc2_app_kit::{NSAppearance, NSAppearanceNameVibrantDark, NSAppearanceNameVibrantLight};
use objc2_foundation::NSString;

pub(crate) fn window_appearance_from_native(appearance: Option<&NSAppearance>) -> WindowAppearance {
    let Some(appearance) = appearance else {
        return WindowAppearance::Light;
    };

    // SAFETY: `-name` returns an autoreleased string; the observed name is
    // stable for the lifetime of the returned appearance.
    let name: Option<Retained<NSString>> = unsafe { msg_send![appearance, name] };
    let Some(name) = name else {
        return WindowAppearance::Light;
    };
    let name_ptr = Retained::as_ptr(&name).cast::<AnyObject>();

    // SAFETY: comparing two object identities never dereferences either.
    unsafe {
        if name_ptr == std::ptr::from_ref(NSAppearanceNameVibrantLight).cast() {
            WindowAppearance::VibrantLight
        } else if name_ptr == std::ptr::from_ref(NSAppearanceNameVibrantDark).cast() {
            WindowAppearance::VibrantDark
        } else if name_ptr == NSAppearanceNameAqua {
            WindowAppearance::Light
        } else if name_ptr == NSAppearanceNameDarkAqua {
            WindowAppearance::Dark
        } else {
            println!(
                "unknown appearance: {:?}",
                (name_ptr as *mut AnyObject).to_str()
            );
            WindowAppearance::Light
        }
    }
}

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    pub static NSAppearanceNameAqua: *mut AnyObject;
    pub static NSAppearanceNameDarkAqua: *mut AnyObject;
}
