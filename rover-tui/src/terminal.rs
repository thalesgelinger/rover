use crossterm::{
    cursor,
    terminal::{self, ClearType},
    ExecutableCommand, QueueableCommand,
};
use std::io::{self, Stdout, Write};

/// Low-level terminal abstraction.
///
/// Manages raw mode, alternate screen, cursor positioning, and buffered writes.
/// All writes are queued into an internal buffer and flushed once per frame
/// to minimize syscalls and prevent partial-frame rendering.
pub struct Terminal {
    stdout: Stdout,
    /// Whether we are currently in raw mode + alternate screen
    active: bool,
    /// Cached terminal dimensions (columns, rows)
    size: (u16, u16),
}

impl Terminal {
    /// Create a new Terminal handle without entering raw mode.
    pub fn new() -> io::Result<Self> {
        let size = terminal::size().unwrap_or((80, 24));
        Ok(Self {
            stdout: io::stdout(),
            active: false,
            size,
        })
    }

    /// Enter the TUI: alternate screen, raw mode, hidden cursor.
    pub fn enter(&mut self) -> io::Result<()> {
        if self.active {
            return Ok(());
        }
        terminal::enable_raw_mode()?;
        self.stdout
            .execute(terminal::EnterAlternateScreen)?;
        self.stdout.execute(cursor::Hide)?;
        self.active = true;
        self.size = terminal::size().unwrap_or(self.size);
        Ok(())
    }

    /// Leave the TUI: restore cursor, disable raw mode, leave alternate screen.
    pub fn leave(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        self.stdout.execute(cursor::Show)?;
        self.stdout
            .execute(terminal::LeaveAlternateScreen)?;
        terminal::disable_raw_mode()?;
        self.active = false;
        Ok(())
    }

    /// Clear the entire screen.
    pub fn clear(&mut self) -> io::Result<()> {
        self.stdout
            .queue(terminal::Clear(ClearType::All))?;
        self.stdout.queue(cursor::MoveTo(0, 0))?;
        Ok(())
    }

    /// Queue a write at a specific (row, col) position.
    /// Does NOT flush — call `flush()` after batching all writes.
    #[inline]
    pub fn queue_write_at(&mut self, row: u16, col: u16, text: &str) -> io::Result<()> {
        self.stdout.queue(cursor::MoveTo(col, row))?;
        self.stdout.write_all(text.as_bytes())?;
        Ok(())
    }

    /// Queue clearing `width` characters starting at (row, col) by overwriting with spaces.
    /// Does NOT flush.
    #[inline]
    pub fn queue_clear_region(&mut self, row: u16, col: u16, width: u16) -> io::Result<()> {
        if width == 0 {
            return Ok(());
        }
        self.stdout.queue(cursor::MoveTo(col, row))?;
        // Write spaces to clear. Avoid allocation for small widths.
        const SPACES: &[u8; 256] = &[b' '; 256];
        let mut remaining = width as usize;
        while remaining > 0 {
            let chunk = remaining.min(SPACES.len());
            self.stdout.write_all(&SPACES[..chunk])?;
            remaining -= chunk;
        }
        Ok(())
    }

    /// Flush all queued writes to the terminal.
    #[inline]
    pub fn flush(&mut self) -> io::Result<()> {
        self.stdout.flush()
    }

    /// Current terminal width in columns.
    #[inline]
    pub fn cols(&self) -> u16 {
        self.size.0
    }

    /// Current terminal height in rows.
    #[inline]
    pub fn rows(&self) -> u16 {
        self.size.1
    }

    /// Refresh cached terminal size. Call on resize events.
    pub fn refresh_size(&mut self) {
        if let Ok(size) = terminal::size() {
            self.size = size;
        }
    }

    /// Whether the terminal is currently in raw/alternate-screen mode.
    #[inline]
    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // Best-effort cleanup — ignore errors during drop
        let _ = self.leave();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_new() {
        let term = Terminal::new();
        assert!(term.is_ok());
        let term = term.unwrap();
        assert!(!term.is_active());
        assert!(term.cols() > 0);
        assert!(term.rows() > 0);
    }

    #[test]
    fn test_terminal_size_defaults() {
        let term = Terminal::new().unwrap();
        // In CI/test environments size may be the fallback (80, 24)
        // but should always be non-zero
        assert!(term.cols() > 0);
        assert!(term.rows() > 0);
    }

    #[test]
    fn test_queue_clear_region_zero_width() {
        // Should be a no-op, no panic
        let mut term = Terminal::new().unwrap();
        let result = term.queue_clear_region(0, 0, 0);
        assert!(result.is_ok());
    }
}
