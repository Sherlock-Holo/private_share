use include_dir::{include_dir, Dir};

pub static WEB_RESOURCES_DIR: Dir<'_> = include_dir!("./ui/build/web");
