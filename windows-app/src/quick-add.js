// The hotkey overlay: one field, one job.
//
// Ctrl+Shift+Space summons it from anywhere. Type a line, press Enter, it is
// gone — capture speed is the entire point, so nothing here ever asks a
// follow-up question.

import { call, clear, el, formatDue, formatTime, invoke, listen, today } from './shared.js';

const line = document.getElementById('line');
const preview = document.getElementById('preview');
let todayDate = new Date();

async function hide() {
  await invoke('hide_window', { label: 'quick-add' });
}

function renderPreview(parsed) {
  clear(preview);
  if (!parsed) return;

  const chips = [];
  if (parsed.title) chips.push(['accent', parsed.title]);
  if (parsed.due) chips.push(['', formatDue(parsed.due, todayDate)]);
  if (parsed.time) chips.push(['', formatTime(parsed.time)]);
  if (parsed.recurrence_label) chips.push(['', `↻ ${parsed.recurrence_label}`]);
  if (parsed.priority && parsed.priority !== 'none') chips.push(['', `! ${parsed.priority}`]);
  if (parsed.project) chips.push(['', `@${parsed.project}`]);
  for (const tag of parsed.tags ?? []) chips.push(['', `#${tag}`]);

  for (const [variant, text] of chips) {
    preview.appendChild(el('span', { class: `chip ${variant}`.trim(), text }));
  }
  for (const guess of parsed.guesses ?? []) {
    preview.appendChild(el('span', { class: 'chip guess', text: guess }));
  }
}

let timer = null;
line.addEventListener('input', () => {
  clearTimeout(timer);
  if (!line.value.trim()) {
    renderPreview(null);
    return;
  }
  timer = setTimeout(async () => {
    renderPreview(await call('preview_line', { line: line.value }, 'Preview'));
  }, 90);
});

line.addEventListener('keydown', async (event) => {
  if (event.key === 'Escape') {
    event.preventDefault();
    hide();
    return;
  }
  if (event.key !== 'Enter') return;

  event.preventDefault();
  const text = line.value.trim();
  if (!text) {
    hide();
    return;
  }
  const added = await call('quick_add', { line: text }, 'Adding task');
  if (!added) return;
  line.value = '';
  renderPreview(null);
  // Shift+Enter keeps the overlay up for a run of quick captures.
  if (!event.shiftKey) hide();
});

// Losing focus means the user moved on; get out of the way.
window.addEventListener('blur', () => hide());

// Re-focus and reset every time the hotkey summons it.
listen('quick-add-opened', async () => {
  todayDate = (await today()) ?? new Date();
  line.value = '';
  renderPreview(null);
  line.focus();
});

(async () => {
  todayDate = (await today()) ?? new Date();
  line.focus();
})();
