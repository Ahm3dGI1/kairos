// The detail pane, Direction B.
//
// A labelled column: what the task is at the top, then its facts as chips, then
// subtasks and the note as two hairline-ruled blocks. The actions live in a
// mono footer keyed like the rest of the app, so the pane teaches its own
// shortcuts instead of hiding them behind icons.

import { clear, el, parseDate, shortTime, toIso } from './shared.js';

const PRIORITIES = [
  ['high', '!p1'],
  ['medium', '!p2'],
  ['low', '!p3'],
  ['none', 'none'],
];

const REPEATS = [
  ['', 'does not repeat'],
  ['every day', 'daily'],
  ['every weekday', 'weekdays'],
  ['every week', 'weekly'],
  ['every other week', 'fortnightly'],
  ['every month', 'monthly'],
  ['every year', 'yearly'],
];

/** Lowercase name, and the single letter the picker shows. */
const DAYS = [
  ['monday', 'M'],
  ['tuesday', 'T'],
  ['wednesday', 'W'],
  ['thursday', 'T'],
  ['friday', 'F'],
  ['saturday', 'S'],
  ['sunday', 'S'],
];

/**
 * The weekdays named in a rule's description.
 *
 * "every Monday and Wednesday" -> ['monday', 'wednesday']. Only a rule that
 * names days at all has any: "every weekday" is a set the core spells out as a
 * word, and pre-selecting M-F from it would turn a preset into a custom rule
 * the moment the popover opened.
 */
function daysIn(label) {
  const words = label.toLowerCase();
  if (!DAYS.some(([name]) => words.includes(name))) return [];
  return DAYS.filter(([name]) => words.includes(name)).map(([name]) => name);
}

function closePops(root) {
  for (const pop of root.querySelectorAll('.pop')) pop.remove();
}

/** Opens a popover anchored under `anchor`, inside the pane. */
function openPop(root, anchor, build) {
  const existing = root.querySelector('.pop');
  const sameOwner = existing?._owner === anchor;
  closePops(root);
  if (sameOwner) return;

  const pop = el('div', { class: 'pop' });
  pop._owner = anchor;
  build(pop, () => closePops(root));
  root.appendChild(pop);

  // Sit under the anchor, clamped inside the pane.
  const paneBox = root.getBoundingClientRect();
  const box = anchor.getBoundingClientRect();
  pop.style.top = `${box.bottom - paneBox.top + 6}px`;
  pop.style.left = `${Math.max(8, Math.min(box.left - paneBox.left, paneBox.width - pop.offsetWidth - 8))}px`;
}

function miniCalendar(selectedIso, todayDate, onPick) {
  const focus = parseDate(selectedIso) ?? todayDate;
  let shown = new Date(focus.getFullYear(), focus.getMonth(), 1);
  const wrap = el('div', { class: 'mini' });

  const draw = () => {
    clear(wrap);
    wrap.appendChild(
      el('div', { class: 'mini-head' }, [
        el('button', { type: 'button', text: '‹', onclick: () => {
          shown = new Date(shown.getFullYear(), shown.getMonth() - 1, 1);
          draw();
        } }),
        el('span', {
          text: shown.toLocaleDateString(undefined, { month: 'short', year: 'numeric' }),
        }),
        el('button', { type: 'button', text: '›', onclick: () => {
          shown = new Date(shown.getFullYear(), shown.getMonth() + 1, 1);
          draw();
        } }),
      ]),
    );

    const grid = el('div', { class: 'mini-grid' });
    for (const day of ['S', 'M', 'T', 'W', 'T', 'F', 'S']) {
      grid.appendChild(el('span', { class: 'mini-dow', text: day }));
    }
    const start = new Date(shown.getFullYear(), shown.getMonth(), 1 - shown.getDay());
    for (let i = 0; i < 42; i += 1) {
      const date = new Date(start);
      date.setDate(start.getDate() + i);
      const iso = toIso(date);
      const classes = ['mini-day'];
      if (date.getMonth() !== shown.getMonth()) classes.push('other');
      if (iso === toIso(todayDate)) classes.push('today');
      if (selectedIso && iso === selectedIso) classes.push('on');
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

/** A hairline-ruled block heading, optionally with a count on the right. */
function blockHead(label, right) {
  return el('div', { class: 'block-head' }, [
    el('span', { text: label }),
    el('span', { class: 'rule' }),
    right ? el('span', { text: right }) : null,
  ]);
}

export function renderDetail(root, task, { todayDate, actions }) {
  clear(root);

  root.appendChild(
    el('div', { class: 'detail-head' }, [
      el('span', { text: 'DETAIL' }),
      el('span', { class: 'spacer' }),
      el('button', {
        type: 'button',
        class: 'plain',
        text: 'Esc',
        'aria-label': 'Close task detail',
        onclick: actions.close,
      }),
    ]),
  );

  const body = el('div', { class: 'detail-body' });

  // ---- what it is ----

  const done = Boolean(task.completed_at);
  const check = el('button', {
    class: `check ${done ? 'on' : ''}`.trim(),
    type: 'button',
    'aria-label': done ? 'Reopen this task' : 'Complete this task',
    onclick: () => actions.toggleComplete(task),
  });

  const title = el('textarea', { class: 'detail-title', rows: '1', spellcheck: 'false' });
  title.value = task.title;
  const autosize = () => {
    title.style.height = 'auto';
    title.style.height = `${title.scrollHeight}px`;
  };
  title.addEventListener('input', autosize);
  title.addEventListener('change', () => actions.edit({ title: title.value.trim() }));
  title.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      title.blur();
    }
  });

  body.appendChild(el('div', { class: 'detail-title-row' }, [check, title]));

  // ---- its facts, as chips ----

  const chips = el('div', { class: 'detail-chips' });
  const dueIso = task.due ?? null;

  const whenLabel = () => {
    if (!dueIso) return 'no date';
    const date = parseDate(dueIso);
    const delta = Math.round((date - todayDate) / 86400000);
    const day =
      delta === 0
        ? 'today'
        : delta === 1
          ? 'tomorrow'
          : date.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short' });
    return task.time ? `${day} ${task.time.slice(0, 5)}` : day;
  };

  const dateChip = el('button', { class: 'chip when', type: 'button', text: whenLabel() });
  dateChip.addEventListener('click', () =>
    openPop(root, dateChip, (pop, close) => {
      pop.appendChild(
        miniCalendar(dueIso, todayDate, (iso) => {
          actions.edit({ due: iso });
          close();
        }),
      );
      const quick = el('div', { class: 'pop-row' });
      const shift = (days) => {
        const date = new Date(todayDate);
        date.setDate(date.getDate() + days);
        return toIso(date);
      };
      for (const [label, iso] of [
        ['today', shift(0)],
        ['tomorrow', shift(1)],
        ['next week', shift(7)],
      ]) {
        quick.appendChild(
          el('button', { type: 'button', text: label, onclick: () => {
            actions.edit({ due: iso });
            close();
          } }),
        );
      }
      pop.appendChild(quick);

      const time = el('input', { type: 'time', value: shortTime(task.time) });
      time.addEventListener('change', () => actions.edit({ time: time.value || null }));
      pop.appendChild(el('label', { class: 'pop-field' }, [el('span', { text: 'time' }), time]));

      pop.appendChild(
        el('div', { class: 'pop-row' }, [
          el('button', { type: 'button', class: 'danger', text: 'clear', onclick: () => {
            actions.edit({ due: null, time: null });
            close();
          } }),
        ]),
      );
    }),
  );
  chips.appendChild(dateChip);

  const repeatChip = el('button', {
    class: 'chip',
    type: 'button',
    text: task.recurrence_label ?? 'no repeat',
  });
  repeatChip.addEventListener('click', () =>
    openPop(root, repeatChip, (pop, close) => {
      // describe() emits exactly these phrases, so the label round-trips.
      const current = task.recurrence_label ?? '';
      for (const [value, label] of REPEATS) {
        pop.appendChild(
          el('button', {
            type: 'button',
            class: `repeat-option ${value === current ? 'on' : ''}`,
            text: label,
            onclick: () => {
              actions.edit({ recurrence: value || null });
              close();
            },
          }),
        );
      }

      // Specific days. The picker composes the same phrase the capture bar
      // would have parsed — "every Monday and Wednesday" — rather than adding
      // a second way to say a rule. describe() and the parser are inverses, so
      // the chosen days come back out of the label they produced.
      const chosen = new Set(daysIn(current));
      const row = el('div', { class: 'day-picker' });
      const send = () => {
        const picked = DAYS.filter(([name]) => chosen.has(name));
        actions.edit({
          recurrence: picked.length ? `every ${picked.map(([n]) => n).join(' and ')}` : null,
        });
      };
      for (const [name, initial] of DAYS) {
        row.appendChild(
          el('button', {
            type: 'button',
            class: `day-key ${chosen.has(name) ? 'on' : ''}`,
            text: initial,
            title: name,
            'aria-pressed': chosen.has(name) ? 'true' : 'false',
            onclick: (event) => {
              const button = event.currentTarget;
              if (chosen.has(name)) chosen.delete(name);
              else chosen.add(name);
              button.classList.toggle('on', chosen.has(name));
              button.setAttribute('aria-pressed', chosen.has(name) ? 'true' : 'false');
              // The popover stays open: picking three days is three clicks.
              send();
            },
          }),
        );
      }
      pop.appendChild(el('div', { class: 'pop-label', text: 'on these days' }));
      pop.appendChild(row);

      if (current && !REPEATS.some(([value]) => value === current) && !chosen.size) {
        pop.appendChild(el('p', { class: 'pop-note', text: `now: ${current}` }));
      }
    }),
  );
  chips.appendChild(repeatChip);

  if (task.project) chips.appendChild(el('span', { class: 'chip', text: `@${task.project}` }));
  for (const tag of task.tags ?? []) {
    chips.appendChild(el('span', { class: 'chip', text: `#${tag}` }));
  }

  const priority = String(task.priority ?? 'none').toLowerCase();
  const prioChip = el('button', {
    class: `chip ${priority !== 'none' ? 'much' : ''}`.trim(),
    type: 'button',
    text: priority === 'none' ? 'no priority' : PRIORITIES.find(([v]) => v === priority)?.[1],
  });
  prioChip.addEventListener('click', () =>
    openPop(root, prioChip, (pop, close) => {
      for (const [value, label] of PRIORITIES) {
        pop.appendChild(
          el('button', {
            type: 'button',
            style: 'display:block;width:100%;text-align:left;margin-bottom:4px',
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
  chips.appendChild(prioChip);
  body.appendChild(chips);

  // ---- subtasks ----

  const subtasks = task.subtasks ?? [];
  const backlog = task.checklist === 'one-per-occurrence';

  // A recurring task can treat its list as a backlog: one item per occurrence,
  // so "Learning" ticks off one thing a week instead of needing all of them.
  const head = blockHead(
    backlog ? 'BACKLOG' : 'SUBTASKS',
    subtasks.length ? `${task.subtasks_done}/${subtasks.length}` : null,
  );
  if (task.recurrence_label) {
    head.insertBefore(
      el('button', {
        type: 'button',
        class: `mode ${backlog ? 'on' : ''}`.trim(),
        title: backlog
          ? 'Each occurrence ticks one item — click for an ordinary checklist'
          : 'Ordinary checklist — click to tick one item per occurrence',
        text: backlog ? 'one per occurrence' : 'all',
        onclick: () =>
          actions.edit({ checklist: backlog ? 'all' : 'one-per-occurrence' }),
      }),
      head.querySelector('.rule').nextSibling,
    );
  }
  const subBlock = el('div', { class: 'block' }, [head]);
  const list = el('ul', { class: 'subtasks' });
  for (const sub of subtasks) {
    list.appendChild(
      el('li', { class: sub.done ? 'done' : '' }, [
        el('button', {
          class: `check ${sub.done ? 'on' : ''}`.trim(),
          type: 'button',
          'aria-label': `${sub.done ? 'Reopen' : 'Complete'}: ${sub.title}`,
          onclick: () => actions.toggleSubtask(task, sub),
        }),
        el('span', { text: sub.title }),
      ]),
    );
  }
  subBlock.appendChild(list);

  const adder = el('input', {
    type: 'text',
    placeholder: backlog ? 'add something to get to' : 'add subtask',
  });
  adder.addEventListener('keydown', async (event) => {
    if (event.key !== 'Enter' || !adder.value.trim()) return;
    await actions.addSubtask(task, adder.value.trim());
    adder.value = '';
  });
  subBlock.appendChild(
    el('div', { class: 'add-row' }, [
      el('span', {}, [
        (() => {
          const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
          svg.setAttribute('width', '10');
          svg.setAttribute('height', '10');
          svg.setAttribute('viewBox', '0 0 24 24');
          svg.setAttribute('fill', 'none');
          svg.setAttribute('stroke', 'currentColor');
          svg.setAttribute('stroke-width', '3');
          svg.setAttribute('stroke-linecap', 'round');
          svg.innerHTML = '<path d="M12 5v14"/><path d="M5 12h14"/>';
          return svg;
        })(),
      ]),
      adder,
    ]),
  );
  body.appendChild(subBlock);

  // ---- the note ----

  const note = el('textarea', { class: 'note-field', placeholder: 'a note…' });
  note.value = task.notes ?? '';
  const sizeNote = () => {
    note.style.height = 'auto';
    note.style.height = `${Math.max(46, note.scrollHeight)}px`;
  };
  note.addEventListener('input', sizeNote);
  note.addEventListener('change', () => actions.edit({ notes: note.value }));
  body.appendChild(el('div', { class: 'block' }, [blockHead('NOTE'), note]));

  root.appendChild(body);

  // ---- the footer: the actions that are not a field ----

  const foot = el('div', { class: 'detail-foot' }, [
    el('button', { type: 'button', text: 'D date', onclick: () => dateChip.click() }),
    el('button', { type: 'button', text: 'R repeat', onclick: () => repeatChip.click() }),
  ]);
  if (task.recurrence_label) {
    const occurrence = task.next ?? task.due;
    foot.append(
      el('button', { type: 'button', text: 'S skip', title: 'Skip this occurrence', onclick: () => actions.skip(task, occurrence) }),
      el('button', { type: 'button', text: 'P push', title: 'Move this occurrence a day', onclick: () => actions.push(task, occurrence) }),
    );
  }
  foot.append(
    el('span', { class: 'spacer' }),
    el('button', { type: 'button', class: 'danger', text: 'Del', onclick: () => actions.remove(task) }),
  );
  root.appendChild(foot);

  // Sizing needs the nodes in the document.
  autosize();
  sizeNote();

  // A click anywhere that is not a chip or a popover dismisses the picker.
  root.addEventListener('mousedown', (event) => {
    if (!event.target.closest('.pop') && !event.target.closest('.chip')) closePops(root);
  });
}
