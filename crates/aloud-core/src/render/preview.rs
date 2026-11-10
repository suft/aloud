//! Spoken previews: short utterances rendered on a dedicated CPU engine so they work during renders.

use super::{RenderService, cache, lock};
use crate::{
    SAMPLE_RATE,
    error::{Error, Result},
    g2p::Dialect,
    models,
    plan::{self, MAX_BATCH_PHONEMES, split_phonemes},
    pron::Dictionary,
    settings::{ModelVariant, Settings},
    tts::{Accel, Engine, EngineOptions, Synth},
};

const MAX_TEXT_CHARS: usize = 400;
const PREVIEW_THREADS: usize = 2;

#[derive(Default)]
pub(super) struct PreviewState {
    engine: Option<(ModelVariant, Engine)>,
}

fn pause_after(batch: &str) -> usize {
    let ms = match batch.trim_end().chars().last() {
        Some('.' | '!' | '?' | '…') => 300,
        Some(',' | ';' | ':') => 150,
        _ => 50,
    };
    ms * SAMPLE_RATE as usize / 1000
}

impl RenderService {
    /// Renders `text` with the voice and the project's (plus global) pronunciation fixes.
    pub fn preview(
        &self,
        project_id: Option<&str>,
        voice: &str,
        speed: f32,
        text: &str,
    ) -> Result<Vec<f32>> {
        let settings = Settings::load(&self.data)?;
        let text: String = text.chars().take(MAX_TEXT_CHARS).collect();
        let book_prons = match project_id {
            Some(id) => self.store.load(id)?.pronunciations,
            None => Vec::new(),
        };
        let dict = Dictionary::new(&settings.pronunciations, &book_prons);
        let applied = dict.apply(&text);
        let phonemes = plan::only_vocab(
            &self
                .phonemizer
                .phonemize_untracked(Dialect::for_voice(voice), &applied)?,
        );
        let batches = split_phonemes(&phonemes, MAX_BATCH_PHONEMES);
        if batches.is_empty() {
            return Err(Error::Invalid("There's nothing to say there.".into()));
        }

        let key = plan::segment_key(settings.model, voice, speed, &batches.join("\u{1}"));
        let dir = self.data.previews();
        if let Ok(pcm) = cache::read_pcm(&dir, &key) {
            return Ok(pcm);
        }

        let voices = self.voices()?;
        let mut state = lock(&self.preview);
        if state
            .engine
            .as_ref()
            .is_none_or(|(m, _)| *m != settings.model)
        {
            let engine = Engine::new(
                &models::model_path(&self.data, settings.model),
                voices,
                EngineOptions {
                    intra_threads: PREVIEW_THREADS,
                    accel: Accel::Cpu,
                },
            )?;
            state.engine = Some((settings.model, engine));
        }
        let engine = &mut state.engine.as_mut().expect("engine just created").1;

        let mut out = Vec::new();
        for (i, b) in batches.iter().enumerate() {
            out.extend(engine.synth(b, voice, speed)?);
            if i + 1 < batches.len() {
                out.resize(out.len() + pause_after(b), 0.0);
            }
        }
        drop(state);
        let _ = cache::write_pcm(&dir, &key, &out);
        Ok(out)
    }
}
