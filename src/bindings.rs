use crate::{Options, Preprocessor as CorePreprocessor};
use std::{fmt, str};
use swc_common::{
    errors::Handler,
    sync::{Lock, Lrc},
    SourceMap, Spanned,
};
use swc_error_reporters::{GraphicalReportHandler, GraphicalTheme, PrettyEmitter};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = Error)]
    fn js_error(message: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = JSON, js_name = parse)]
    fn json_parse(value: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = Reflect, js_name = get)]
    fn js_get(obj: JsValue, field_name: &str) -> JsValue;

    #[wasm_bindgen(js_name = Boolean)]
    fn js_boolean(value: JsValue) -> bool;
}

#[wasm_bindgen]
extern "C" {
    pub type JsOptions;

    #[wasm_bindgen(method, getter, structural)]
    fn inline_source_map(this: &JsOptions) -> bool;

    #[wasm_bindgen(method, getter, structural)]
    fn transformer(this: &JsOptions) -> Option<js_sys::Function>;
}

#[wasm_bindgen]
pub struct Preprocessor {
    options: Lrc<Options>,
}

#[derive(Clone, Default)]
struct Writer(Lrc<Lock<String>>);

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.lock().write_str(s)
    }
}

fn capture_err_detail(
    err: swc_ecma_parser::error::Error,
    source_map: Lrc<SourceMap>,
    theme: GraphicalTheme,
) -> JsValue {
    let wr = Writer::default();
    let emitter = PrettyEmitter::new(
        source_map,
        Box::new(wr.clone()),
        GraphicalReportHandler::new_themed(theme),
        Default::default(),
    );
    let handler = Handler::with_emitter(true, false, Box::new(emitter));
    err.into_diagnostic(&handler).emit();
    let s = wr.0.lock().as_str().to_string();
    s.into()
}

fn as_javascript_error(err: swc_ecma_parser::error::Error, source_map: Lrc<SourceMap>) -> JsValue {
    let short_desc = format!("Parse Error at {}", source_map.span_to_string(err.span()));
    let js_err = js_error(short_desc.into());
    js_sys::Reflect::set(
        &js_err,
        &"source_code".into(),
        &capture_err_detail(
            err.clone(),
            source_map.clone(),
            GraphicalTheme::unicode_nocolor(),
        ),
    )
    .unwrap();
    js_sys::Reflect::set(
        &js_err,
        &"source_code_color".into(),
        &capture_err_detail(err, source_map, GraphicalTheme::unicode()),
    )
    .unwrap();
    return js_err;
}

impl Into<Lrc<Options>> for JsOptions {
    fn into(self) -> Lrc<Options> {
        Lrc::new(Options {
            inline_source_map: self.inline_source_map(),
            transformer: self.transformer().map(convert_js_transformer),
        })
    }
}

fn convert_js_transformer(js_function: js_sys::Function) -> Box<dyn Fn(String) -> String> {
    let null = JsValue::null();
    Box::new(move |s: String| -> String {
        let js_in_string = JsValue::from(s);
        let result = js_function
            .call1(&null, &js_in_string)
            .and_then(|v| {
                v.as_string().ok_or(js_error(
                    "transformer function didn't return a string".into(),
                ))
            });

        match result {
            Ok(js_string) => js_string.into(),
            Err(err) => panic!("nope")
        }
    })
}

#[wasm_bindgen]
impl Preprocessor {
    #[wasm_bindgen(constructor)]
    pub fn new(options: Option<JsOptions>) -> Self {
        Self {
            options: match options {
                Some(o) => o.into(),
                None => Default::default(),
            },
        }
    }

    pub fn process(&self, src: String, filename: Option<String>) -> Result<String, JsValue> {
        let preprocessor = CorePreprocessor::new(self.options.clone());
        let result = preprocessor.process(&src, filename.map(|f| f.into()));

        match result {
            Ok(output) => Ok(output),
            Err(err) => Err(as_javascript_error(err, preprocessor.source_map()).into()),
        }
    }

    pub fn parse(&self, src: String, filename: Option<String>) -> Result<JsValue, JsValue> {
        let preprocessor = CorePreprocessor::new(self.options.clone());
        let result = preprocessor
            .parse(&src, filename.as_ref().map(|f| f.into()))
            .map_err(|_err| self.process(src, filename).unwrap_err())?;
        let serialized = serde_json::to_string(&result)
            .map_err(|err| js_error(format!("Unexpected serialization error; please open an issue with the following debug info: {err:#?}").into()))?;
        Ok(json_parse(serialized.into()))
    }
}
