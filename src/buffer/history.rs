use super::Buffer;

impl Buffer {
    pub fn push_history(&mut self) {
        self.history.push(self.lines.clone());
        if self.history.len() > 100 {
            self.history.remove(0);
        }
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.history.pop() {
            self.redo_stack.push(self.lines.clone());
            self.lines = prev;
            if self.lines.is_empty() {
                self.lines.push(String::new());
            }
            self.clamp_cursor();
            self.anchor = self.cursor;
            self.modified = true;
            self.needs_reparse = true;
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo_stack.pop() {
            self.history.push(self.lines.clone());
            self.lines = next;
            if self.lines.is_empty() {
                self.lines.push(String::new());
            }
            self.clamp_cursor();
            self.anchor = self.cursor;
            self.modified = true;
            self.needs_reparse = true;
            true
        } else {
            false
        }
    }
}
