//! A selected document's editing state, independent of its on-disk file.
use super::{ColumnBlock, FindState, Selection, Session, Undo, ViewState};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use wordcraft_doc::props::CharProps;
use wordcraft_doc::{Document, Pos};
#[derive(Serialize, Deserialize)]
pub struct WorkspaceCheckpoint {
    #[serde(with = "full_document")]
    doc: Document,
    sel: Selection,
    view: ViewState,
    pending: Option<CharProps>,
    path: Option<std::path::PathBuf>,
    file_name: Option<String>,
    dirty: bool,
    find: FindState,
    goal_x: Option<f32>,
    page_hint: usize,
    history: Vec<Arc<Undo>>,
    redo: Vec<Arc<Undo>>,
    rev: u64,
    id: u64,
    untitled: u32,
    made_rev: u64,
    column: Option<ColumnBlock>,
    column_mode: bool,
    also_selected: Vec<Pos>,
    edit_marks: Vec<Pos>,
    go_back_step: usize,
    versions: Vec<VersionCheckpoint>,
    password: Option<String>,
    originals: std::collections::HashMap<String, String>,
    picture_looks: std::collections::HashMap<String, crate::cmd::objects::PictureLook>,
    merge: crate::cmd::mailings::MergeState,
    math: Option<super::MathEdit>,
    math_latex: bool,
    math_normal_text: bool,
    new_header_parts: Vec<u32>,
    hf_return: Option<Selection>,
}
impl Session {
    pub fn workspace_checkpoint(&self) -> WorkspaceCheckpoint {
        WorkspaceCheckpoint {
            doc: self.doc.clone(),
            sel: self.sel.clone(),
            view: self.view.clone(),
            pending: self.pending.clone(),
            path: self.path.clone(),
            file_name: self.file_name.clone(),
            dirty: self.dirty,
            find: self.find.clone(),
            goal_x: self.goal_x,
            page_hint: self.page_hint,
            history: self.history.clone(),
            redo: self.redo.clone(),
            rev: self.rev,
            id: self.document_id,
            untitled: self.untitled,
            made_rev: self.made_rev,
            column: self.column.clone(),
            column_mode: self.column_mode,
            also_selected: self.also_selected.clone(),
            edit_marks: self.edit_marks.clone(),
            go_back_step: self.go_back_step,
            versions: self
                .versions
                .iter()
                .map(|(label, date, doc)| VersionCheckpoint { label: label.clone(), date: date.clone(), doc: doc.clone() })
                .collect(),
            password: self.password.as_ref().map(|p| p.as_str().to_string()),
            originals: self.originals.clone(),
            picture_looks: self.picture_looks.clone(),
            merge: self.merge.clone(),
            math: self.math.clone(),
            math_latex: self.math_latex,
            math_normal_text: self.math_normal_text,
            new_header_parts: self.new_header_parts.clone(),
            hf_return: self.hf_return.clone(),
        }
    }
    pub fn workspace_import(&mut self, c: WorkspaceCheckpoint) -> Result<(), String> {
        if self.documents().len() != 1 || self.dirty || self.path.is_some() || !self.history.is_empty() || c.id == 0 || c.id >= u64::MAX - 1024 {
            return Err("Destination is not an empty editor".into());
        }
        self.set_document(c.doc);
        self.sel = c.sel;
        self.view = c.view;
        self.pending = c.pending;
        self.path = c.path;
        self.file_name = c.file_name;
        self.dirty = c.dirty;
        self.find = c.find;
        self.goal_x = c.goal_x;
        self.page_hint = c.page_hint;
        self.history = c.history;
        self.redo = c.redo;
        self.rev = c.rev;
        self.document_id = c.id;
        self.order = vec![c.id];
        self.untitled = c.untitled;
        self.untitled_count = self.untitled_count.max(c.untitled);
        self.made_rev = c.made_rev;
        self.column = c.column;
        self.column_mode = c.column_mode;
        self.also_selected = c.also_selected;
        self.edit_marks = c.edit_marks;
        self.go_back_step = c.go_back_step;
        self.versions = c.versions.into_iter().map(|v| (v.label, v.date, v.doc)).collect();
        self.password = c.password.map(|p| crate::io::Password::new(&p));
        self.originals = c.originals;
        self.picture_looks = c.picture_looks;
        self.merge = c.merge;
        self.math = c.math;
        self.math_latex = c.math_latex;
        self.math_normal_text = c.math_normal_text;
        self.new_header_parts = c.new_header_parts;
        self.hf_return = c.hf_return;
        if let Some(math) = &mut self.math {
            math.session = crate::next_math_session().ok_or("Equation session exhausted")?;
        }
        self.typing_open = false;
        super::documents::NEXT_DOCUMENT_ID.fetch_max(c.id + 1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
    pub fn workspace_commit(&mut self, id: u64, revision: u64) -> Result<(), String> {
        let valid = if self.document_id == id { self.rev == revision } else { self.others.iter().any(|d| d.document_id == id && d.rev == revision) };
        if !valid || self.documents().len() < 2 {
            return Err("The source changed; the original was kept".into());
        }
        self.close_document(id).map_err(|e| e.to_string())
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn aw_selected_word_checkpoint_keeps_unknown_markup() {
        let mut doc = wordcraft_doc::Document::new();
        let rev = doc.para(wordcraft_doc::StoryRef::Body, &wordcraft_doc::Path::top(0)).unwrap().rev;
        let mut xml = wordcraft_doc::kept::KeptXml { format: "synthetic".into(), ..Default::default() };
        xml.twin_wraps.insert((7, rev), (vec![std::sync::Arc::from("<original>")], 1));
        doc.kept = wordcraft_doc::kept::Kept::new(xml);
        let source = super::Session::new(doc);
        let bytes = serde_json::to_vec(&source.workspace_checkpoint()).unwrap();
        let mut dest = super::Session::new(wordcraft_doc::Document::new());
        dest.workspace_import(serde_json::from_slice(&bytes).unwrap()).unwrap();
        let new_rev = dest.doc.para(wordcraft_doc::StoryRef::Body, &wordcraft_doc::Path::top(0)).unwrap().rev;
        assert_eq!(dest.doc.kept.get().unwrap().format, "synthetic");
        assert!(dest.doc.kept.get().unwrap().twin_wraps.contains_key(&(7, new_rev)));
    }
    #[test]
    fn aw_selected_word_checkpoint_rejects_a_changed_source() {
        let mut s = super::Session::new(wordcraft_doc::Document::new());
        s.add_document(wordcraft_doc::Document::new());
        let id = s.document_id();
        let revision = s.rev();
        s.run("text.insert", &serde_json::json!({"text":"continua na origem"})).unwrap();
        assert!(s.workspace_commit(id, revision).is_err());
        assert_eq!(s.documents().len(), 2);
    }
    #[test]
    fn aw_selected_word_checkpoint_preserves_content_selection_and_undo() {
        let mut source = super::Session::new(wordcraft_doc::Document::new());
        source.run("text.insert", &serde_json::json!({"text":"Primeiro"})).unwrap();
        let first = source.document_id();
        source.add_document(wordcraft_doc::Document::new());
        source.run("text.insert", &serde_json::json!({"text":"Selecionado"})).unwrap();
        let id = source.document_id();
        let rev = source.rev();
        let selection = source.sel.clone();
        let bytes = serde_json::to_vec(&source.workspace_checkpoint()).unwrap();
        let mut dest = super::Session::new(wordcraft_doc::Document::new());
        dest.workspace_import(serde_json::from_slice(&bytes).unwrap()).unwrap();
        source.workspace_commit(id, rev).unwrap();
        assert_eq!(source.documents().len(), 1);
        assert_eq!(source.document_id(), first);
        assert_eq!(dest.document_id(), id);
        assert_eq!(dest.sel, selection);
        assert!(dest.dirty);
        dest.run("edit.undo", &serde_json::json!({})).unwrap();
        assert_eq!(dest.doc.word_count(), 0);
    }
}

#[derive(Serialize, Deserialize)]
struct VersionCheckpoint {
    label: String,
    date: String,
    #[serde(with = "full_document")]
    doc: Document,
}
/// Ordinary native JSON omits reader-kept markup. A workspace move must keep it.
pub(crate) mod full_document {
    use serde::{Deserialize, Serialize};
    use wordcraft_doc::{Document, StoryRef};
    #[derive(Serialize)]
    struct Borrowed<'a> {
        doc: &'a Document,
        kept: Vec<u8>,
        missing: &'a wordcraft_doc::MissingPictures,
        revisions: Vec<u64>,
    }
    #[derive(Deserialize)]
    struct Owned {
        doc: Document,
        kept: Vec<u8>,
        missing: wordcraft_doc::MissingPictures,
        revisions: Vec<u64>,
    }
    fn revisions(doc: &Document) -> Vec<u64> {
        std::iter::once(StoryRef::Body)
            .chain(doc.parts.keys().copied().map(StoryRef::Part))
            .flat_map(|story| doc.para_paths(story).into_iter().filter_map(move |path| doc.para(story, &path).map(|p| p.rev)))
            .collect()
    }
    pub fn serialize<S: serde::Serializer>(doc: &Document, serializer: S) -> Result<S::Ok, S::Error> {
        let kept = postcard::to_allocvec(&doc.kept).map_err(serde::ser::Error::custom)?;
        Borrowed { doc, kept, missing: &doc.missing_pictures, revisions: revisions(doc) }.serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Document, D::Error> {
        let mut value = Owned::deserialize(deserializer)?;
        let kept: wordcraft_doc::kept::Kept = postcard::from_bytes(&value.kept).map_err(serde::de::Error::custom)?;
        let ids: std::collections::HashMap<u64, u64> = value.revisions.into_iter().zip(revisions(&value.doc)).collect();
        if let Some(old) = kept.get() {
            let mut xml = old.clone();
            xml.twin_wraps = xml.twin_wraps.into_iter().filter_map(|((fp, rev), v)| ids.get(&rev).map(|fresh| ((fp, *fresh), v))).collect();
            xml.pending_wraps = xml.pending_wraps.into_iter().map(|(k, v)| (*ids.get(&k).unwrap_or(&k), v)).collect();
            xml.pending_indexes = xml.pending_indexes.into_iter().map(|(k, v)| (*ids.get(&k).unwrap_or(&k), v)).collect();
            xml.pending_groups = xml.pending_groups.into_iter().map(|(k, v)| (*ids.get(&k).unwrap_or(&k), v)).collect();
            xml.pending = xml.pending.into_iter().map(|(k, v)| (*ids.get(&k).unwrap_or(&k), v)).collect();
            value.doc.kept = wordcraft_doc::kept::Kept::new(xml);
        }
        value.doc.missing_pictures = value.missing;
        Ok(value.doc)
    }
}
