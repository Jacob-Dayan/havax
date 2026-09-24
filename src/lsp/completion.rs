use crate::lsp::CompletionItem;

pub struct CompletionMenu {
    pub items: Vec<CompletionItem>,
    pub selected_idx: usize,
    pub trigger_col: usize,
    pub prefix: String,
    pub scroll_offset: usize,
    pub visible: bool,
}

impl Default for CompletionMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl CompletionMenu {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selected_idx: 0,
            trigger_col: 0,
            prefix: String::new(),
            scroll_offset: 0,
            visible: false,
        }
    }

    pub fn show(&mut self, trigger_col: usize, prefix: &str, items: Vec<CompletionItem>) {
        if items.is_empty() {
            self.visible = false;
            return;
        }
        self.trigger_col = trigger_col;
        self.prefix = prefix.to_string();
        self.items = items;
        self.selected_idx = 0;
        self.scroll_offset = 0;
        self.visible = true;
    }

    pub fn update_prefix(&mut self, prefix: &str, candidates: Vec<CompletionItem>) {
        self.prefix = prefix.to_string();
        self.items = candidates;
        if self.items.is_empty() {
            self.visible = false;
        } else {
            self.visible = true;
            if self.selected_idx >= self.items.len() {
                self.selected_idx = 0;
            }
        }
    }

    pub fn select_next(&mut self, visible_count: usize) {
        if self.items.is_empty() {
            return;
        }
        if self.selected_idx + 1 < self.items.len() {
            self.selected_idx += 1;
            if self.selected_idx >= self.scroll_offset + visible_count {
                self.scroll_offset = self.selected_idx - visible_count + 1;
            }
        } else {
            // Wrap to top
            self.selected_idx = 0;
            self.scroll_offset = 0;
        }
    }

    pub fn select_prev(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
            if self.selected_idx < self.scroll_offset {
                self.scroll_offset = self.selected_idx;
            }
        } else {
            // Wrap to bottom
            self.selected_idx = self.items.len().saturating_sub(1);
            let visible_count = 10;
            if self.selected_idx >= visible_count {
                self.scroll_offset = self.selected_idx - visible_count + 1;
            } else {
                self.scroll_offset = 0;
            }
        }
    }

    pub fn selected_item(&self) -> Option<&CompletionItem> {
        if self.visible && self.selected_idx < self.items.len() {
            Some(&self.items[self.selected_idx])
        } else {
            None
        }
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.items.clear();
        self.prefix.clear();
        self.selected_idx = 0;
        self.scroll_offset = 0;
    }
}
