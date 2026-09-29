use super::Buffer;

impl Buffer {
    pub fn reparse(&mut self) {
        let lang_name = self.language().to_string();
        if lang_name != "rust" && lang_name != "toml" {
            self.tree = None;
            self.needs_reparse = false;
            return;
        }

        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.lines.hash(&mut hasher);
        let current_hash = hasher.finish();

        if let Some(last_hash) = self.last_parsed_hash
            && last_hash == current_hash
            && self.tree.is_some()
            && !self.needs_reparse
        {
            return;
        }
        self.last_parsed_hash = Some(current_hash);
        self.needs_reparse = false;

        self.cached_text = Some(self.lines.join("\n"));
        let full_text = self.cached_text.as_deref().unwrap_or("");
        if let Some(lang) = crate::grammar::try_load_language(&lang_name) {
            let mut parser = tree_sitter::Parser::new();
            if parser.set_language(&lang).is_ok() {
                self.tree = parser.parse(full_text, None);
            } else {
                self.tree = None;
            }
        } else {
            self.tree = None;
        }
    }

    pub fn apply_tree_edit(&mut self, edit: &tree_sitter::InputEdit) {
        if let Some(tree) = &mut self.tree {
            tree.edit(edit);
        }
    }

    pub fn reparse_incremental(&mut self) {
        let Some(lang_name) = self.language.as_deref() else {
            self.tree = None;
            self.needs_reparse = false;
            return;
        };

        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.lines.hash(&mut hasher);
        let current_hash = hasher.finish();
        self.last_parsed_hash = Some(current_hash);
        self.needs_reparse = false;

        self.cached_text = Some(self.lines.join("\n"));
        let full_text = self.cached_text.as_deref().unwrap_or("");
        if let Some(lang) = crate::grammar::try_load_language(lang_name) {
            let mut parser = tree_sitter::Parser::new();
            if parser.set_language(&lang).is_ok() {
                self.tree = parser.parse(full_text, self.tree.as_ref());
            } else {
                self.tree = None;
            }
        } else {
            self.tree = None;
        }
    }
}
