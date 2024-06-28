use anyhow::Context as _;
use wasm_bindgen::prelude::*;

use sekken_converter::kana::KanaTable;
use sekken_core::Sekken;
use sekken_dict::BTreeDict;
use sekken_model::CompactModel;
use sekken_segmenter::SKK;

use sekken_lattice::{Dict, SegmentConverter};

thread_local! {
    static SEKKEN: Sekken = Sekken::new();
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    pub fn alert(s: &str);
}

#[wasm_bindgen]
pub fn init() {
    console_error_panic_hook::set_once();
}

fn replace_segconverter(map: Box<dyn SegmentConverter>) {
    SEKKEN.with(|sekken| {
        sekken.replace_segconverter(map);
    });
}

#[wasm_bindgen]
pub fn use_default_segconverter() -> Result<(), JsError> {
    replace_segconverter(Box::new(SKK::<KanaTable>::default()));
    return Ok(());
}

fn replace_skk_dictionary(dict: Box<dyn Dict>) {
    SEKKEN.with(|sekken| {
        sekken.replace_dictionary(dict);
    });
}

#[wasm_bindgen]
pub fn set_skk_dictionary(dict: &[u8]) -> Result<(), JsError> {
    let dict = BTreeDict::from_skk_json(dict).context("parse dictionary json");
    match dict {
        Ok(dict) => {
            replace_skk_dictionary(Box::new(dict));
            Ok(())
        }
        Err(e) => Err(JsError::new(&format!("{:?}", e))),
    }
}

#[wasm_bindgen]
pub fn set_model(data: &[u8]) -> Result<(), JsError> {
    let model = CompactModel::load(data).context("failed to load model");
    match model {
        Ok(model) => {
            SEKKEN.with(|sekken| {
                sekken.replace_model(Box::new(model));
            });
            Ok(())
        }
        Err(e) => Err(JsError::new(&format!("{:?}", e))),
    }
}

#[wasm_bindgen]
pub fn henkan(roman: String, n: usize) -> Result<Vec<String>, JsError> {
    let result = SEKKEN.with(|sekken| sekken.henkan(&roman, n).context("failed to henkan"));
    match result {
        Ok(result) => Ok(result),
        Err(e) => Err(JsError::new(&format!("{:?}", e))),
    }
}
