use crossterm::{
    cursor,
    terminal::{self, ClearType},
    ExecutableCommand, QueueableCommand,
};
use std::io::{self, Stdout, Write};

/// How the terminal was entered.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TerminalMode {
    Inactive,
    /// Inline: renders at current cursor position, no alternate screen.
    Inline,
    /// Fullscreen: alternate screen buffer.
    Fullscreen,
}

/// Low-level terminal abstraction.
///
/// Supports two modes:
/// - **Inline** (default): renders in-place at the current cursor position,
///   like a CLI progress bar. No alternate screen.
/// - **Fullscreen**: alternate screen buffer, clears everything.
///
/// All writes are queued and flushed once per frame to minimize syscalls.
pub struct Terminal {
    stdout: Stdout,
    mode: TerminalMode,
    /// Cached terminal dimensions (columns, rows)
    size: (u16, u16),
    /// Absolute row where the rendered content starts (inline mode).
    origin_row: u16,
    /// Height of rendered content in rows (inline mode).
    content_height: u16,
}

impl Terminal {
    pub fn new() -> io::Result<Self> {
        let size = terminal::size().unwrap_or((80, 24));
        Ok(Self {
            stdout: io::stdout(),
            mode: TerminalMode::Inactive,
            size,
            origin_row: 0,
            content_height: 0,
        })
    }

    /// Enter inline mode: raw mode, hide cursor, reserve vertical space.
    ///
    /// Scrolls the terminal if needed to make room for `content_height` rows,
    /// then positions the cursor at the top of the reserved region.
    pub fn enter_inline(&mut self, content_height: u16) -> io::Result<()> {
        if self.mode != TerminalMode::Inactive {
            return Ok(());
        }

        terminal::enable_raw_mode()?;
        self.size = terminal::size().unwrap_or(self.size);

        // Reserve vertical space by emitting newlines.
        // This scrolls the terminal if we're near the bottom.
        let needed = content_height.max(1);
        for _ in 0..needed {
            self.stdout.write_all(b"\r\n")?;
        }
        self.stdout.flush()?;

        // Query where the cursor ended up after scrolling
        let (_, cursor_row) = cursor::position()?;
        self.origin_row = cursor_row.saturating_sub(needed.saturating_sub(1));
        self.content_height = content_height;

        self.stdout.execute(cursor::Hide)?;
        self.mode = TerminalMode::Inline;
        Ok(())
    }

    /// Enter fullscreen mode: alternate screen, raw mode, hidden cursor.
    pub fn enter_fullscreen(&mut self) -> io::Result<()> {
        if self.mode != TerminalMode::Inactive {
            return Ok(());
        }
        terminal::enable_raw_mode()?;
        self.stdout.execute(terminal::EnterAlternateScreen)?;
        self.stdout.execute(cursor::Hide)?;
        self.mode = TerminalMode::Fullscreen;
        self.size = terminal::size().unwrap_or(self.size);
        Ok(())
    }

    /// Leave whichever mode is active, restoring the terminal.
    pub fn leave(&mut self) -> io::Result<()> {
        match self.mode {
            TerminalMode::Inactive => Ok(()),
            TerminalMode::Inline => {
                // Move cursor below rendered content
                let end_row = self.origin_row + self.content_height;
                self.stdout.execute(cursor::MoveTo(0, end_row))?;
                self.stdout.execute(cursor::Show)?;
                self.stdout.write_all(b"\r\n")?;
                self.stdout.flush()?;
                terminal::disable_raw_mode()?;
                self.mode = TerminalMode::Inactive;
                Ok(())
            }
            TerminalMode::Fullscreen => {
                self.stdout.execute(cursor::Show)?;
                self.stdout.execute(terminal::LeaveAlternateScreen)?;
                terminal::disable_raw_mode()?;
                self.mode = TerminalMode::Inactive;
                Ok(())
            }
        }
    }

    /// Clear the entire screen (fullscreen mode only).
    pub fn clear(&mut self) -> io::Result<()> {
        self.stdout.queue(terminal::Clear(ClearType::All))?;
        self.stdout.queue(cursor::MoveTo(0, 0))?;
        Ok(())
    }

    /// Clear only the inline render region by overwriting with spaces.
    pub fn clear_inline_region(&mut self) -> io::Result<()> {
        let cols = self.size.0;
        for row in 0..self.content_height {
            self.queue_clear_region(self.origin_row + row, 0, cols)?;
        }
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

    /// Absolute screen row where rendered content starts (inline mode).
    #[inline]
    pub fn origin_row(&self) -> u16 {
        self.origin_row
    }

    /// Refresh cached terminal size. Call on resize events.
    pub fn refresh_size(&mut self) {
        if let Ok(size) = terminal::size() {
            self.size = size;
        }
    }

    /// Show the terminal cursor at an absolute (row, col) position.
    pub fn show_cursor_at(&mut self, row: u16, col: u16) -> io::Result<()> {
        self.stdout.execute(cursor::MoveTo(col, row))?;
        self.stdout.execute(cursor::Show)?;
        Ok(())
    }

    /// Hide the terminal cursor.
    pub fn hide_cursor(&mut self) -> io::Result<()> {
        self.stdout.execute(cursor::Hide)?;
        Ok(())
    }

    /// Whether the terminal is currently active.
    #[inline]
    pub fn is_active(&self) -> bool {
        self.mode != TerminalMode::Inactive
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
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
        assert!(term.cols() > 0);
        assert!(term.rows() > 0);
    }

    #[test]
    fn test_queue_clear_region_zero_width() {
        let mut term = Terminal::new().unwrap();
        let result = term.queue_clear_region(0, 0, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_origin_row_defaults_to_zero() {
        let term = Terminal::new().unwrap();
        assert_eq!(term.origin_row(), 0);
    }
}
