import { createCapture } from './capture.js';
import { applyTheme, el, invoke, listen, today } from './shared.js';

const input = document.getElementById('line');
const previewNode = document.getElementById('preview');
let todayDate = new Date();
let keepOpen = false;
let settings = {};

async function hide() {
  await invoke('hide_window', { label: 'quick-add' });
}

const capture = createCapture({
  input,
  layer: document.getElementById('highlight'),
  previewNode,
  getToday: () => todayDate,
  onAdd: () => {
    if (!keepOpen && !settings.hotkey_stays_open) hide();
    keepOpen = false;
  },
});

// The keys this window answers to, shown where the preview row ends.
function addHint() {
  if (previewNode.querySelector('.hint')) return;
  previewNode.appendChild(
    el('span', { class: 'hint', text: 'Enter add · Shift+Enter keep open · Esc dismiss' }),
  );
}
new MutationObserver(addHint).observe(previewNode, { childList: true });

input.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') {
    event.preventDefault();
    hide();
    return;
  }
  if (event.key === 'Enter') keepOpen = event.shiftKey;
  if (event.key.toLowerCase() === 'r' && event.ctrlKey) {
    event.preventDefault();
    capture.restore();
  }
});

// Losing focus means the user moved on; get out of the way.
window.addEventListener('blur', () => hide());

listen('quick-add-opened', async () => {
  todayDate = (await today()) ?? new Date();
  capture.reset();
  capture.focus();
});

(async () => {
  todayDate = (await today()) ?? new Date();
  capture.focus();
})();

async function loadSettings() {
  settings = await invoke('settings_values');
  applyTheme(settings);
  capture.setPills(settings.parse_pills !== false);
}
listen('settings-changed', loadSettings);
loadSettings();
