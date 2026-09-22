import { call, clear, el, formatDue, formatTime } from './shared.js';

const escapeHtml = (text) =>
  text.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);

/** Which of the three tints a field takes. */
const TINT = {
  date: 'when',
  time: 'when',
  recurrence: 'when',
  tag: 'tag',
  project: 'project',
  priority: 'priority',
};

export function createCapture({ input, layer, previewNode, getToday, onAdd, onJournal }) {
  /** Byte ranges the user reverted, kept until the text under them changes. */
  let excluded = [];
  /** The most recently reverted range, so R can put it back. */
  let lastReverted = null;
  let spans = [];
  let lastValue = input.value;
  let mode = 'task';
  let timer = null;

  const encoder = new TextEncoder();

  /** The parser speaks bytes; JS string indexes are UTF-16. */
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

  // Off, the field is plain text. The parse still runs — the preview line
  // under the bar still says what was understood — only the tints go.
  let pills = true;

  function renderLayer(text) {
    if (!pills) {
      layer.textContent = text;
      return;
    }
    const marks = [
      ...spans.map((s) => ({ ...s, off: false })),
      ...excluded.map(([start, end]) => ({ start, end, field: null, off: true })),
    ].sort((a, b) => a.start - b.start);

    if (!marks.length) {
      layer.textContent = text;
      return;
    }
    let html = '';
    let cursor = 0;
    for (const mark of marks) {
      const start = byteToIndex(text, mark.start);
      const end = byteToIndex(text, mark.end);
      if (start < cursor || end > text.length) continue;
      html += escapeHtml(text.slice(cursor, start));
      const cls = mark.off ? 'tok tok-off' : `tok tok-${TINT[mark.field] ?? 'when'}`;
      html += `<span class="${cls}">${escapeHtml(text.slice(start, end))}</span>`;
      cursor = end;
    }
    html += escapeHtml(text.slice(cursor));
    layer.innerHTML = html;
  }

  function renderPreview(preview) {
    clear(previewNode);
    if (mode === 'journal' || !preview) return;
    const today = getToday();

    if (preview.title) {
      previewNode.appendChild(el('span', { class: 'as-title', text: preview.title }));
    }

    // What it is scheduled for leads, because that is what the line changed.
    const when = [];
    if (preview.due) when.push(formatDue(preview.due, today).toLowerCase());
    if (preview.time) when.push(formatTime(preview.time));
    if (when.length) {
      previewNode.appendChild(el('span', { class: 'chip when', text: when.join(' ') }));
    }
    if (preview.recurrence_label) {
      previewNode.appendChild(el('span', { class: 'chip', text: preview.recurrence_label }));
    }
    const where = [];
    if (preview.project) where.push(`@${preview.project}`);
    for (const tag of preview.tags ?? []) where.push(`#${tag}`);
    if (where.length) {
      previewNode.appendChild(el('span', { class: 'chip', text: where.join(' ') }));
    }
    if (preview.priority && preview.priority !== 'none') {
      previewNode.appendChild(el('span', { class: 'chip much', text: preview.priority }));
    }
    for (const guess of preview.guesses ?? []) {
      previewNode.appendChild(el('span', { class: 'chip guess', text: `guessed — ${guess}` }));
    }
    if (excluded.length) {
      previewNode.appendChild(
        el('span', { class: 'chip reverted' }, [
          el('span', { text: `${excluded.length} reverted` }),
          lastReverted
            ? el('button', { type: 'button', text: 'R restores', onclick: restore })
            : null,
        ]),
      );
    }
  }

  async function reparse() {
    const text = input.value;
    if (!text.trim() || mode === 'journal') {
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
    // The layer has to track the text at once, or the tints lag a keystroke.
    renderLayer(input.value);
  }

  /**
   * Keeps reverted ranges over the same words after an edit: ranges before it
   * are untouched, ranges after it shift, and a range the edit cut into is
   * dropped, because it no longer describes what the user reverted.
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
      .map(([start, end]) => (start >= removedEnd ? [start + delta, end + delta] : [start, end]));
    lastReverted = null;
  }

  function restore() {
    if (!lastReverted) return;
    excluded = excluded.filter(
      ([start, end]) => !(start === lastReverted[0] && end === lastReverted[1]),
    );
    lastReverted = null;
    reparse();
    input.focus();
  }

  input.addEventListener('input', () => {
    shiftExclusions(lastValue, input.value);
    lastValue = input.value;
    schedule();
  });

  input.addEventListener('scroll', () => {
    layer.scrollLeft = input.scrollLeft;
  });

  input.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      submit(event.shiftKey);
      return;
    }
    if (event.key !== 'Backspace' || input.selectionStart !== input.selectionEnd) return;

    // Backspace against a tint takes the parse back rather than deleting a
    // character — the same gesture that undoes an autoformat in a document.
    const caret = indexToByte(input.value, input.selectionStart);
    const hit = spans.find((span) => span.start < caret && caret <= span.end);
    if (!hit) return;

    event.preventDefault();
    excluded.push([hit.start, hit.end]);
    lastReverted = [hit.start, hit.end];
    reparse();
  });

  async function submit(keepOpen = false) {
    const line = input.value.trim();
    if (!line) return;

    if (mode === 'journal') {
      await onJournal?.(line);
      reset();
      return;
    }
    const task = await call('quick_add', { line, excluded }, 'Adding task');
    if (!task) return;
    reset();
    onAdd?.(task, keepOpen);
  }

  function reset() {
    input.value = '';
    lastValue = '';
    excluded = [];
    lastReverted = null;
    spans = [];
    renderLayer('');
    renderPreview(null);
  }

  return {
    submit,
    reset,
    refresh: reparse,
    restore,
    focus: () => input.focus(),
    setMode: (next) => {
      if (next === mode) return;
      mode = next;
      reset();
    },
    setPills: (next) => {
      if (next === pills) return;
      pills = next;
      renderLayer(input.value);
    },
  };
}
