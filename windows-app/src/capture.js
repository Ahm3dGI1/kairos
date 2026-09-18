// The capture field.
//
// Two jobs beyond holding text: showing which phrases the parser claimed, and
// letting the user take any of them back. A phrase the parser recognized gets a
// tinted pill; pressing backspace against one un-parses it, the way an editor
// undoes an autoformat, and the words drop back into the title.
//
// The pills are painted by a mirror layer sitting behind a transparent input,
// which is the only way to decorate ranges inside a plain <input>.

import { call, clear, el, formatDue, formatTime } from './shared.js';

const escapeHtml = (text) =>
  text.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);

export function createCapture({ input, layer, previewNode, getToday, onAdd }) {
  /** Byte ranges the user reverted, kept until the text under them changes. */
  let excluded = [];
  /** The spans the parser most recently claimed. */
  let spans = [];
  let lastValue = input.value;
  let timer = null;

  // The layer mirrors the input's text, so byte offsets and pixels line up.
  // JS string indexes are UTF-16; the backend speaks bytes. For the ASCII this
  // field mostly sees they agree, and the encoder below keeps them honest.
  const encoder = new TextEncoder();

  /** Converts a byte offset from the parser into a JS string index. */
  function byteToIndex(text, byteOffset) {
    if (byteOffset <= 0) return 0;
    let bytes = 0;
    for (let i = 0; i < text.length; i += 1) {
      if (bytes >= byteOffset) return i;
      bytes += encoder.encode(text[i]).length;
    }
    return text.length;
  }

  function indexToByte(text, index) {
    return encoder.encode(text.slice(0, index)).length;
  }

  function renderLayer(text) {
    if (!spans.length) {
      layer.textContent = text;
      return;
    }
    const ordered = [...spans].sort((a, b) => a.start - b.start);
    let html = '';
    let cursor = 0;
    for (const span of ordered) {
      const start = byteToIndex(text, span.start);
      const end = byteToIndex(text, span.end);
      if (start < cursor) continue;
      html += escapeHtml(text.slice(cursor, start));
      html += `<mark class="tok tok-${span.field}">${escapeHtml(text.slice(start, end))}</mark>`;
      cursor = end;
    }
    html += escapeHtml(text.slice(cursor));
    layer.innerHTML = html;
  }

  function renderPreview(preview) {
    clear(previewNode);
    if (!preview) return;
    const today = getToday();

    const chips = [];
    if (preview.title) chips.push(['accent', preview.title]);
    if (preview.due) chips.push(['', formatDue(preview.due, today)]);
    if (preview.time) chips.push(['', formatTime(preview.time)]);
    if (preview.recurrence_label) chips.push(['', `↻ ${preview.recurrence_label}`]);
    if (preview.priority && preview.priority !== 'none') chips.push(['', `! ${preview.priority}`]);
    if (preview.project) chips.push(['', `@${preview.project}`]);
    for (const tag of preview.tags ?? []) chips.push(['', `#${tag}`]);

    for (const [variant, text] of chips) {
      previewNode.appendChild(el('span', { class: `chip ${variant}`.trim(), text }));
    }
    for (const guess of preview.guesses ?? []) {
      previewNode.appendChild(el('span', { class: 'chip guess', text: guess }));
    }
    if (excluded.length) {
      previewNode.appendChild(
        el('span', { class: 'chip muted', text: `${excluded.length} reverted` }),
      );
    }
  }

  async function reparse() {
    const text = input.value;
    if (!text.trim()) {
      spans = [];
      renderLayer(text);
      renderPreview(null);
      return;
    }
    const preview = await call('preview_line', { line: text, excluded }, 'Preview');
    spans = preview?.spans ?? [];
    renderLayer(text);
    renderPreview(preview);
  }

  function schedule() {
    clearTimeout(timer);
    timer = setTimeout(reparse, 90);
    // The layer has to track the text immediately, or it lags a keystroke
    // behind and the pills visibly slide.
    renderLayer(input.value);
  }

  /**
   * Keeps reverted ranges pointing at the same words after an edit: ranges
   * before the edit are untouched, ranges after it shift, and a range the edit
   * cut into is dropped, because it no longer describes what the user reverted.
   */
  function shiftExclusions(before, after) {
    let prefix = 0;
    while (prefix < before.length && prefix < after.length && before[prefix] === after[prefix]) {
      prefix += 1;
    }
    let suffix = 0;
    while (
      suffix < before.length - prefix &&
      suffix < after.length - prefix &&
      before[before.length - 1 - suffix] === after[after.length - 1 - suffix]
    ) {
      suffix += 1;
    }
    const editStart = indexToByte(before, prefix);
    const removedEnd = indexToByte(before, before.length - suffix);
    const delta = encoder.encode(after).length - encoder.encode(before).length;

    excluded = excluded
      .filter(([start, end]) => end <= editStart || start >= removedEnd)
      .map(([start, end]) =>
        start >= removedEnd ? [start + delta, end + delta] : [start, end],
      );
  }

  input.addEventListener('input', () => {
    shiftExclusions(lastValue, input.value);
    lastValue = input.value;
    schedule();
  });

  // Keep the pills aligned when the text scrolls past the field's width.
  input.addEventListener('scroll', () => {
    layer.scrollLeft = input.scrollLeft;
  });

  input.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      submit();
      return;
    }
    if (event.key !== 'Backspace' || input.selectionStart !== input.selectionEnd) return;

    // Backspace against a pill takes the parse back instead of deleting a
    // character — the same gesture that undoes an autoformat in a document.
    const caret = indexToByte(input.value, input.selectionStart);
    const hit = spans.find((span) => span.start < caret && caret <= span.end);
    if (!hit) return;

    event.preventDefault();
    excluded.push([hit.start, hit.end]);
    reparse();
  });

  async function submit() {
    const line = input.value.trim();
    if (!line) return;
    const task = await call('quick_add', { line, excluded }, 'Adding task');
    if (!task) return;
    reset();
    onAdd?.(task);
  }

  function reset() {
    input.value = '';
    lastValue = '';
    excluded = [];
    spans = [];
    renderLayer('');
    renderPreview(null);
  }

  return { submit, reset, refresh: reparse, focus: () => input.focus() };
}
