pub trait Model: Sized {
    const MODEL_NAME: &'static str;
    const PRIMARY_KEY: &'static str;
    type Patch;
}
