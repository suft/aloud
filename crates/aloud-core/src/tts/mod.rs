//! Kokoro synthesis on ONNX Runtime.

mod trim;

use ort::{
    environment::Environment,
    ep,
    session::{Session, builder::GraphOptimizationLevel},
    value::Tensor,
};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, LazyLock},
};

use crate::{
    error::{Error, Result},
    voices::Voices,
};

pub use trim::trim;

static VOCAB: LazyLock<HashMap<char, i64>> = LazyLock::new(|| {
    let raw: HashMap<String, i64> =
        serde_json::from_str(include_str!("vocab.json")).expect("vocab.json is valid");
    raw.into_iter()
        .filter_map(|(k, v)| {
            let mut it = k.chars();
            match (it.next(), it.next()) {
                (Some(c), None) => Some((c, v)),
                _ => None,
            }
        })
        .collect()
});

/// Phoneme character → token id.
pub fn vocab() -> &'static HashMap<char, i64> {
    &VOCAB
}

static ENV: LazyLock<std::result::Result<Environment, String>> = LazyLock::new(|| {
    ort::init()
        .with_name("aloud")
        .build()
        .map_err(|e| e.to_string())
});

fn env() -> Result<&'static Environment> {
    ENV.as_ref()
        .map_err(|e| Error::Other(format!("Couldn't start the speech engine: {e}")))
}

fn ort_err(e: impl std::fmt::Display) -> Error {
    Error::Other(format!("Speech engine error: {e}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accel {
    Cpu,
    Gpu,
}

#[derive(Debug, Clone, Copy)]
pub struct EngineOptions {
    pub intra_threads: usize,
    pub accel: Accel,
}

/// Whether this platform has a GPU execution provider that Aloud tries to use.
pub fn gpu_supported() -> bool {
    cfg!(any(target_os = "macos", target_os = "windows"))
}

pub struct Engine {
    session: Session,
    voices: Arc<Voices>,
}

impl Engine {
    pub fn new(model: &Path, voices: Arc<Voices>, opts: EngineOptions) -> Result<Self> {
        let mut builder = Session::builder(env()?)
            .map_err(ort_err)?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(ort_err)?
            .with_intra_threads(opts.intra_threads.max(1))
            .map_err(ort_err)?
            .with_inter_threads(1)
            .map_err(ort_err)?;
        if opts.accel == Accel::Gpu {
            builder = builder
                .with_execution_providers([gpu_provider(), ep::CPU::default().build()])
                .map_err(ort_err)?;
        }
        let session = builder.commit_from_file(model).map_err(ort_err)?;
        Ok(Engine { session, voices })
    }

    /// Opens an engine. With `Accel::Gpu` the engine is verified with a short utterance and
    /// replaced by a CPU engine if it fails; the bool reports that fallback.
    pub fn open(model: &Path, voices: Arc<Voices>, opts: EngineOptions) -> Result<(Self, bool)> {
        if opts.accel == Accel::Cpu {
            return Ok((Self::new(model, voices, opts)?, false));
        }
        let healthy = Self::new(model, voices.clone(), opts)
            .ok()
            .and_then(|mut e| match e.synth("hˈɛlO.", "af_heart", 1.0) {
                Ok(a) if !a.is_empty() && a.iter().all(|x| x.is_finite()) => Some(e),
                _ => None,
            });
        match healthy {
            Some(e) => Ok((e, false)),
            None => {
                let cpu = EngineOptions {
                    accel: Accel::Cpu,
                    ..opts
                };
                Ok((Self::new(model, voices, cpu)?, true))
            }
        }
    }

    /// Synthesizes one utterance (at most ~510 phoneme ids) and trims leading/trailing silence.
    pub fn synth(&mut self, phonemes: &str, voice: &str, speed: f32) -> Result<Vec<f32>> {
        let vocab = vocab();
        let mut tokens: Vec<i64> = vec![0];
        tokens.extend(phonemes.chars().filter_map(|c| vocab.get(&c).copied()));
        let n_ids = tokens.len() - 1;
        if n_ids == 0 {
            return Ok(Vec::new());
        }
        tokens.push(0);
        let style = self
            .voices
            .style(voice, n_ids)
            .ok_or_else(|| Error::Invalid(format!("Unknown voice: {voice}")))?
            .to_vec();

        let outputs = self
            .session
            .run(ort::inputs![
                "tokens" => Tensor::from_array(([1usize, tokens.len()], tokens)).map_err(ort_err)?,
                "style" => Tensor::from_array(([1usize, 256], style)).map_err(ort_err)?,
                "speed" => Tensor::from_array(([1usize], vec![speed])).map_err(ort_err)?,
            ])
            .map_err(ort_err)?;
        let (_, audio) = outputs["audio"]
            .try_extract_tensor::<f32>()
            .map_err(ort_err)?;
        Ok(trim(audio).to_vec())
    }
}

fn gpu_provider() -> ort::ep::ExecutionProviderDispatch {
    #[cfg(target_os = "macos")]
    return ep::CoreML::default().build();
    #[cfg(target_os = "windows")]
    return ep::DirectML::default().build();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    unreachable!("no GPU execution provider on this platform; `gpu_supported()` is false")
}
