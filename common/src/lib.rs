pub mod auth;
pub mod format;
pub mod language;
pub mod serialization;
pub mod templates;
pub mod types;

pub mod i18n;

#[cfg(feature = "server")]
pub mod database;

#[doc(hidden)]
pub use iconify;

#[macro_export]
macro_rules! icon {
     ($name:literal) => {
         $crate::icon!($name class="")
     };
     ($name:literal $( $attr:literal = $val:literal)*) => {
         $crate::icon!($name class="" $($attr = $val)*)
     };
     ($name:literal class=$class:literal $( $attr:literal = $val:literal)*) => {
         concat!(
             "<span class='material-icons",
             " ",
             $class,
             "'",
             " ",
             $(
                $attr,
                "=",
                $val,
                " ",
             )*
             ">",
             $crate::iconify::svg!($name, color = "currentColor"),
             "</span>"
         )
     };

}
