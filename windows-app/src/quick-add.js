// The hotkey overlay: one field, one job.
//
// Ctrl+Shift+Space summons it from anywhere. Type a line, press Enter, it is
// gone — capture speed is the entire point, so nothing here ever asks a
// follow-up question.
//
// It shares its field with the main window, so the parsed phrases get the same
// pills and the same backspace-to-revert.

import { createCapture } from './capture.js';
import { invoke, listen, today } from './shared.js';

const input = document.getElementById('line');
let todayDate = new Date();

async function hide() {
  await invoke('hide_window', { label: 'quick-add' });
}

const capture = createCapture({
  input,
  layer: document.getElementById('highlight'),
  previewNode: document.getElementById('preview'),
  getToday: () => todayDate,
  onAdd: () => {
    // Shift was held for a run of captures, so stay open for the next one.
    if (!keepOpen) hide();
    keepOpen = false;
  },
});

// Enter is handled inside the capture field; this only records whether the
// overlay should survive it.
let keepOpen = false;
input.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') {
    event.preventDefault();
    hide();
    return;
  }
  if (event.key === 'Enter') keepOpen = event.shiftKey;
});

// Losing focus means the user moved on; get out of the way.
window.addEventListener('blur', () => hide());

// Re-focus and reset every time the hotkey summons it.
listen('quick-add-opened', async () => {
  todayDate = (await today()) ?? new Date();
  capture.reset();
  capture.focus();
});

(async () => {
  todayDate = (await today()) ?? new Date();
  capture.focus();
})();
