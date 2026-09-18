// The task detail pane.
//
// Minimal on purpose: two chips, a title, and one body that is either notes or
// subtasks. Everything else — the calendar, the time, the repeat rule, the
// priority — lives behind a chip, so the pane shows what a task *is* and only
// unfolds a picker when you ask it to.

import { call, clear, el, formatDue, parseDate, shortTime, toIso } from './shared.js';

const PRIORITIES = [
  ['high', 'High', 'p-high'],
  ['medium', 'Medium', 'p-medium'],
  ['low', 'Low', 'p-low'],
  ['none', 'None', ''],
];

const REPEATS = [
  ['', 'Does not repeat'],
  ['every day', 'Daily'],
  ['every weekday', 'Every weekday'],
  ['every week', 'Weekly'],
  ['every other week', 'Every other week'],
  ['every month', 'Monthly'],
  ['every year', 'Yearly'],
];

/** Closes whatever popover is open, if any. */
function closePopovers(root) {
  for (const open of root.querySelectorAll('.popover')) open.remove();
  for (const active of root.querySelectorAll('.chip-button.open')) active.classList.remove('open');
}

/** Anchors a popover under `button` inside the pane. */
function openPopover(root, button, build) {
  const wasOpen = button.classList.contains('open');
  closePopovers(root);
  if (wasOpen) return;

  button.classList.add('open');
  const popover = el('div', { class: 'popover' });
  build(popover, () => closePopovers(root));
  button.parentElement.appendChild(popover);
}

/** A month grid for the date picker. */
function miniCalendar(selectedIso, todayDate, onPick) {
  const selected = parseDate(selectedIso);
  const focus = selected ?? todayDate;
  let shown = new Date(focus.getFullYear(), focus.getMonth(), 1);

  const wrap = el('div', { class: 'mini-cal' });

  const draw = () => {
    clear(wrap);
    const label = shown.toLocaleDateString(undefined, { month: 'long', year: 'numeric' });
    wrap.appendChild(
      el('div', { class: 'mini-head' }, [
        el('button', {
          type: 'button',
          text: '‹',
          onclick: () => {
            shown = new Date(shown.getFullYear(), shown.getMonth() - 1, 1);
            draw();
          },
        }),
        el('strong', { text: label }),
        el('button', {
          type: 'button',
          text: '›',
          onclick: () => {
            shown = new Date(shown.getFullYear(), shown.getMonth() + 1, 1);
            draw();
          },
        }),
      ]),
    );

    const grid = el('div', { class: 'mini-grid' });
    for (const day of ['M', 'T', 'W', 'T', 'F', 'S', 'S']) {
      grid.appendChild(el('span', { class: 'mini-dow', text: day }));
    }
    // Monday-first, like the rest of the app.
    const lead = (shown.getDay() + 6) % 7;
    const start = new Date(shown.getFullYear(), shown.getMonth(), 1 - lead);
    for (let i = 0; i < 42; i += 1) {
      const date = new Date(start);
      date.setDate(start.getDate() + i);
      const iso = toIso(date);
      const classes = ['mini-day'];
      if (date.getMonth() !== shown.getMonth()) classes.push('other');
      if (iso === toIso(todayDate)) classes.push('today');
      if (selectedIso && iso === selectedIso) classes.push('selected');
      grid.appendChild(
        el('button', {
          type: 'button',
          class: classes.join(' '),
          text: String(date.getDate()),
          onclick: () => onPick(iso),
        }),
      );
    }
    wrap.appendChild(grid);
  };

  draw();
  return wrap;
}

/**
 * Renders the pane for `task` into `root`.
 *
 * `actions` supplies everything that touches the store, so this module stays
 * about layout and never learns how a task is saved.
 */
export function renderDetail(root, task, { todayDate, actions, bodyMode, onBodyMode }) {
  clear(root);
  closePopovers(root);

  // ---- chips row: date, priority, close ----

  const dueIso = task.due ?? null;
  const dateLabel = dueIso ? formatDue(dueIso, todayDate) : 'No date';
  const dateChip = el('button', {
    class: `chip-button ${dueIso ? 'set' : ''}`.trim(),
    type: 'button',
    title: 'Date, time and repeat',
  }, [el('span', { class: 'chip-icon', text: '🗓' }), el('span', { text: dateLabel })]);

  dateChip.addEventListener('click', () =>
    openPopover(root, dateChip, (popover, close) => {
      popover.appendChild(
        miniCalendar(dueIso, todayDate, (iso) => {
          actions.edit({ due: [iso] });
          close();
        }),
      );

      const quick = el('div', { class: 'popover-row' });
      const shift = (days) => {
        const date = new Date(todayDate);
        date.setDate(date.getDate() + days);
        return toIso(date);
      };
      for (const [label, iso] of [
        ['Today', shift(0)],
        ['Tomorrow', shift(1)],
        ['Next week', shift(7)],
      ]) {
        quick.appendChild(
          el('button', {
            type: 'button',
            text: label,
            onclick: () => {
              actions.edit({ due: [iso] });
              close();
            },
          }),
        );
      }
      popover.appendChild(quick);

      // Time and repeat live with the date because they only mean anything
      // once something is scheduled.
      const time = el('input', { type: 'time', value: shortTime(task.time) });
      time.addEventListener('change', () => actions.edit({ time: [time.value || null] }));
      popover.appendChild(
        el('label', { class: 'popover-field' }, [el('span', { text: 'Time' }), time]),
      );

      const repeat = el('select');
      // describe() emits exactly the phrases listed in REPEATS, so the label
      // doubles as the picker's value and a round trip changes nothing.
      const current = task.recurrence_label ?? '';
      for (const [value, label] of REPEATS) {
        const option = el('option', { value, text: label });
        if (value === current) option.selected = true;
        repeat.appendChild(option);
      }
      repeat.addEventListener('change', () => actions.edit({ recurrence: [repeat.value || null] }));
      popover.appendChild(
        el('label', { class: 'popover-field' }, [el('span', { text: 'Repeat' }), repeat]),
      );
      // A rule the picker cannot express is shown rather than silently reset.
      if (task.recurrence_label && !REPEATS.some(([value]) => value === current)) {
        popover.appendChild(
          el('p', { class: 'popover-note', text: `Currently: ${task.recurrence_label}` }),
        );
      }

      popover.appendChild(
        el('div', { class: 'popover-row' }, [
          el('button', {
            type: 'button',
            class: 'danger',
            text: 'Clear date',
            onclick: () => {
              actions.edit({ due: [null], time: [null] });
              close();
            },
          }),
        ]),
      );
    }),
  );

  const priorityName = String(task.priority ?? 'none').toLowerCase();
  const priorityChip = el(
    'button',
    {
      class: `chip-button flag ${priorityName !== 'none' ? `set p-${priorityName}` : ''}`.trim(),
      type: 'button',
      title: 'Priority',
    },
    [
      el('span', { class: 'chip-icon', text: '⚑' }),
      el('span', { text: priorityName === 'none' ? 'Priority' : PRIORITIES.find(([v]) => v === priorityName)?.[1] ?? 'Priority' }),
    ],
  );
  priorityChip.addEventListener('click', () =>
    openPopover(root, priorityChip, (popover, close) => {
      for (const [value, label, cls] of PRIORITIES) {
        popover.appendChild(
          el('button', {
            type: 'button',
            class: `popover-item ${cls}`.trim(),
            text: label,
            onclick: () => {
              actions.edit({ priority: value });
              close();
            },
          }),
        );
      }
    }),
  );

  root.appendChild(
    el('div', { class: 'detail-chips' }, [
      dateChip,
      priorityChip,
      el('button', {
        class: 'icon-button close',
        type: 'button',
        title: 'Close  (Esc)',
        'aria-label': 'Close task detail',
        text: '✕',
        onclick: actions.close,
      }),
    ]),
  );

  // ---- title, with the body toggle beside it ----

  const title = el('textarea', { class: 'detail-title', rows: '1', spellcheck: 'false' });
  title.value = task.title;
  const autosize = () => {
    title.style.height = 'auto';
    title.style.height = `${title.scrollHeight}px`;
  };
  title.addEventListener('input', autosize);
  title.addEventListener('change', () => actions.edit({ title: title.value.trim() }));
  // Enter commits rather than adding a line; the title is one line by nature.
  title.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      title.blur();
    }
  });

  const subtaskCount = (task.subtasks ?? []).length;
  const toggle = el('button', {
    class: 'icon-button',
    type: 'button',
    title: bodyMode === 'notes' ? 'Switch to subtasks' : 'Switch to notes',
    'aria-label': bodyMode === 'notes' ? 'Switch to subtasks' : 'Switch to notes',
    text: bodyMode === 'notes' ? '☑' : '¶',
    onclick: () => onBodyMode(bodyMode === 'notes' ? 'subtasks' : 'notes'),
  });

  root.appendChild(
    el('div', { class: 'detail-title-row' }, [
      title,
      el('div', { class: 'title-tools' }, [
        subtaskCount
          ? el('span', {
              class: 'count',
              text: `${task.subtasks_done}/${subtaskCount}`,
            })
          : null,
        toggle,
      ]),
    ]),
  );
  // Sizing needs the element in the document.
  autosize();

  root.appendChild(
    el('div', { class: 'detail-body' }, [
      bodyMode === 'notes' ? notesBody(task, actions) : subtaskBody(task, actions),
    ]),
  );

  // ---- the few actions that are not a field ----

  const footer = el('div', { class: 'detail-actions' }, [
    el('button', {
      type: 'button',
      text: task.completed_at ? 'Reopen' : 'Complete',
      onclick: () => actions.toggleComplete(task),
    }),
  ]);
  if (task.recurrence_label) {
    const occurrence = task.next ?? task.due;
    footer.append(
      el('button', {
        type: 'button',
        text: 'Skip one',
        title: 'Skip this occurrence, keep the series',
        onclick: () => actions.skip(task, occurrence),
      }),
      el('button', {
        type: 'button',
        text: 'Push a day',
        title: 'Move just this occurrence to tomorrow',
        onclick: () => actions.push(task, occurrence),
      }),
    );
  }
  footer.appendChild(
    el('button', {
      type: 'button',
      class: 'danger',
      text: 'Delete',
      onclick: () => actions.remove(task),
    }),
  );
  root.appendChild(footer);

  // A click anywhere else in the pane dismisses an open picker.
  root.addEventListener('mousedown', (event) => {
    if (!event.target.closest('.popover') && !event.target.closest('.chip-button')) {
      closePopovers(root);
    }
  });
}

function notesBody(task, actions) {
  const notes = el('textarea', {
    class: 'detail-notes',
    placeholder: 'Notes — markdown, bullet points, anything.',
  });
  notes.value = task.notes ?? '';
  notes.addEventListener('change', () => actions.edit({ notes: notes.value }));
  return notes;
}

function subtaskBody(task, actions) {
  const list = el('ul', { class: 'subtasks' });
  for (const sub of task.subtasks ?? []) {
    const box = el('input', { type: 'checkbox' });
    box.checked = sub.done;
    box.addEventListener('change', () => actions.toggleSubtask(task, sub));

    const text = el('span', { text: sub.title });
    list.appendChild(el('li', { class: sub.done ? 'done' : '' }, [box, text]));
  }

  const adder = el('input', {
    type: 'text',
    class: 'inline-input',
    placeholder: 'Add a subtask…',
  });
  adder.addEventListener('keydown', async (event) => {
    if (event.key !== 'Enter' || !adder.value.trim()) return;
    await actions.addSubtask(task, adder.value.trim());
    adder.value = '';
  });

  return el('div', {}, [list, adder]);
}

/** Re-exported so the caller can dismiss pickers when the pane closes. */
export { closePopovers };
