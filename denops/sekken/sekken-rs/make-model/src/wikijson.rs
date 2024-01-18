use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct WikiJSON {
    pub text: String,
}
