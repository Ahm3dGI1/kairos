// Master Todo — main window.
//
// Three pages: Tasks (one list grouped by when things are due), Calendar, and
// Habits (a month grid with the month's journal). Capture belongs to the first
// two; the habit month has nothing to capture into.

import { createCapture } from './capture.js';
import { renderDetail } from './detail.js';
import { createHabitMonth } from './habits.js';
import {
  call,
  clear,
  el,
  formatDue,
  formatTime,
  listen,
  parseDate,
  priorityClass,
  shortTime,
  toIso,
  today,
} from './shared.js';

const PAGES = ['agenda', 'calendar', 'habits'];

const state = {
  page: 'agenda',
  search: '',
  project: null,
  tag: null,
  showCompleted: false,
  groups: [],
  rows: [],
  cursor: 0,
  detailId: null,
  /** Whether the detail pane is showing notes or subtasks. */
  bodyMode: 'notes',
  todayDate: new Date(),
  month: null,
  projects: [],
  tags: [],
};

const dom = {};
for (const id of [
  'capture', 'quick-add', 'highlight', 'add-button', 'preview', 'filters', 'projects',
  'tags', 'search', 'show-completed', 'agenda', 'empty', 'calendar', 'detail', 'summary',
  'toast', 'page-agenda', 'page-calendar', 'page-habits', 'agenda-page', 'calendar-page',
  'habits-page', 'month-prev', 'month-next', 'month-today', 'month-label', 'habit-grid',
  'new-habit', 'journal-entries', 'add-entry',
]) {
  dom[id] = document.getElementById(id);
}

// ---------- capture ----------

const capture = createCapture({
  input: dom['quick-add'],
  layer: dom.highlight,
  previewNode: dom.preview,
  getToday: () => state.todayDate,
  onAdd: (task) => {
    showToast(`Added “${task.title}”`);
    refresh();
  },
});

// ---------- habits ----------

const habitMonth = createHabitMonth({
  grid: dom['habit-grid'],
  newHabit: dom['new-habit'],
  entriesNode: dom['journal-entries'],
  addEntry: dom['add-entry'],
  label: dom['month-label'],
  getToday: () => state.todayDate,
});

// ---------- data ----------

async function refresh() {
  state.todayDate = (await today()) ?? new Date();

  if (state.page === 'habits') {
    await habitMonth.load();
  } else {
    await loadAgenda();
    if (state.page === 'calendar') await renderCalendar();
  }

  state.projects = (await call('projects', undefined, 'Projects')) ?? [];
  state.tags = (await call('tags', undefined, 'Tags')) ?? [];
  renderSidebar();

  dom.summary.textContent = (await call('summary', undefined, 'Summary')) ?? '';
}

async function loadAgenda() {
  const groups = await call(
    'agenda',
    {
      query: {
        include_completed: state.showCompleted,
        search: state.search || null,
        project: state.project,
        tag: state.tag,
      },
    },
    'Loading tasks',
  );
  state.groups = groups ?? [];
  state.rows = state.groups.flatMap((group) => group.tasks);
  if (state.cursor >= state.rows.length) state.cursor = Math.max(0, state.rows.length - 1);
  renderAgenda();
  renderPane();
}

// ---------- the task list ----------

function renderAgenda() {
  clear(dom.agenda);

  if (!state.rows.length) {
    dom.empty.hidden = false;
    dom.empty.textContent = state.search
      ? 'Nothing matches that search.'
      : 'Nothing ahead. Press N to add something.';
    return;
  }
  dom.empty.hidden = true;

  // One flat index across the groups, so J/K walks the whole list.
  let index = 0;
  for (const group of state.groups) {
    dom.agenda.appendChild(
      el('li', { class: `group-head group-${group.id}` }, [
        el('span', { text: group.label }),
        el('span', { class: 'count', text: String(group.tasks.length) }),
      ]),
    );
    for (const task of group.tasks) {
      dom.agenda.appendChild(taskRow(task, index, group.id));
      index += 1;
    }
  }
}

function taskRow(task, index, groupId) {
  const done = Boolean(task.completed_at);
  const classes = [
    'task',
    priorityClass(task.priority),
    done ? 'done' : '',
    index === state.cursor ? 'active' : '',
  ];

  const check = el('button', {
    class: 'check',
    type: 'button',
    title: done ? 'Mark not done' : 'Complete',
    text: '✓',
    onclick: (event) => {
      event.stopPropagation();
      toggleComplete(task);
    },
  });

  const meta = [];
  const dueIso = task.next ?? task.due;
  // The group heading already says "Today"; repeating it per row is noise.
  if (dueIso && !['today', 'tomorrow'].includes(groupId)) {
    meta.push(
      el('span', {
        class: task.overdue ? 'overdue' : '',
        text: formatDue(dueIso, state.todayDate),
      }),
    );
  }
  if (task.time) meta.push(el('span', { text: formatTime(shortTime(task.time)) }));
  if (task.recurrence_label) meta.push(el('span', { text: `↻ ${task.recurrence_label}` }));
  if (task.project) meta.push(el('span', { text: `@${task.project}` }));
  for (const tag of task.tags ?? []) meta.push(el('span', { class: 'tag', text: `#${tag}` }));
  if (task.subtasks?.length) {
    meta.push(el('span', { text: `${task.subtasks_done}/${task.subtasks.length}` }));
  }

  const main = el('div', { class: 'task-main' }, [
    el('span', { class: 'title', text: task.title }),
    meta.length ? el('div', { class: 'meta' }, meta) : null,
  ]);

  return el(
    'li',
    {
      class: classes.filter(Boolean).join(' '),
      onclick: () => {
        state.cursor = index;
        openDetail(task.id);
      },
    },
    [check, main],
  );
}

function renderSidebar() {
  // The saved filters are the ones that are not about time — time is what the
  // list's own headings are for.
  clear(dom.filters);
  dom.filters.appendChild(
    el(
      'li',
      {
        class: !state.project && !state.tag ? 'selected' : '',
        onclick: () => {
          state.project = null;
          state.tag = null;
          refresh();
        },
      },
      [el('span', { text: 'Everything' })],
    ),
  );

  renderNamedList(dom.projects, state.projects, 'project', '');
  renderNamedList(dom.tags, state.tags, 'tag', '#');
}

function renderNamedList(node, names, kind, prefix) {
  clear(node);
  for (const name of names) {
    const selected = state[kind] === name;
    node.appendChild(
      el('li', {
        class: selected ? 'selected' : '',
        text: prefix + name,
        // Clicking the active one clears it, so a filter is never a trap.
        onclick: () => {
          state[kind] = selected ? null : name;
          state.cursor = 0;
          refresh();
        },
      }),
    );
  }
}

// ---------- the detail pane ----------

function openDetail(id) {
  state.detailId = id;
  renderAgenda();
  renderPane();
}

function closeDetail() {
  state.detailId = null;
  dom.detail.hidden = true;
  renderAgenda();
  dom.agenda.focus();
}

function renderPane() {
  const task = state.rows.find((t) => t.id === state.detailId);
  if (!task || state.page !== 'agenda') {
    dom.detail.hidden = true;
    return;
  }
  dom.detail.hidden = false;

  // Open on whichever body the task actually has something in.
  const mode = state.bodyMode ?? 'notes';
  renderDetail(dom.detail, task, {
    todayDate: state.todayDate,
    bodyMode: mode,
    onBodyMode: (next) => {
      state.bodyMode = next;
      renderPane();
    },
    actions: {
      close: closeDetail,
      edit: (changes) => edit(task.id, changes),
      toggleComplete,
      remove,
      skip: async (t, date) => {
        await call('skip_occurrence', { id: t.id, date }, 'Skip');
        showToast('Occurrence skipped');
        refresh();
      },
      push: async (t, date) => {
        const next = parseDate(date);
        next.setDate(next.getDate() + 1);
        await call('reschedule_occurrence', { id: t.id, date, to: toIso(next) }, 'Reschedule');
        showToast('Occurrence moved to the next day');
        refresh();
      },
      toggleSubtask: async (t, sub) => {
        await call('toggle_subtask', { id: t.id, subtaskId: sub.id }, 'Subtask');
        refresh();
      },
      addSubtask: async (t, title) => {
        await call('add_subtask', { id: t.id, title }, 'Subtask');
        await refresh();
      },
    },
  });
}

// ---------- calendar ----------

async function renderCalendar() {
  const [year, month] = state.month ?? (await call('current_month', undefined, 'Calendar')) ?? [];
  if (!year) return;
  state.month = [year, month];

  const days = (await call('calendar_month', { year, month }, 'Calendar')) ?? [];
  const byDate = new Map(days.map((d) => [d.date, d.tasks]));

  clear(dom.calendar);
  const label = new Date(year, month - 1, 1).toLocaleDateString(undefined, {
    month: 'long',
    year: 'numeric',
  });
  dom.calendar.appendChild(
    el('div', { class: 'cal-head' }, [
      el('button', { type: 'button', text: '‹', onclick: () => stepMonth(-1) }),
      el('strong', { text: label }),
      el('button', { type: 'button', text: '›', onclick: () => stepMonth(1) }),
      el('button', {
        type: 'button',
        text: 'Today',
        onclick: () => {
          state.month = null;
          renderCalendar();
        },
      }),
    ]),
  );

  const grid = el('div', { class: 'cal-grid' });
  for (const name of ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']) {
    grid.appendChild(el('div', { class: 'cal-dow', text: name }));
  }

  const first = new Date(year, month - 1, 1);
  // Monday-first grid: JS weeks start on Sunday, so shift by one.
  const lead = (first.getDay() + 6) % 7;
  const start = new Date(year, month - 1, 1 - lead);
  const todayIso = toIso(state.todayDate);

  for (let i = 0; i < 42; i += 1) {
    const date = new Date(start);
    date.setDate(start.getDate() + i);
    const iso = toIso(date);
    const cell = el('div', {
      class: [
        'cal-day',
        date.getMonth() === month - 1 ? '' : 'other',
        iso === todayIso ? 'today' : '',
      ]
        .filter(Boolean)
        .join(' '),
    });
    cell.appendChild(el('div', { class: 'cal-date', text: String(date.getDate()) }));
    for (const task of byDate.get(iso) ?? []) {
      cell.appendChild(
        el('div', {
          class: 'cal-task',
          text: task.title,
          title: task.title,
          onclick: () => {
            // Jump back to the list, where the detail pane lives.
            state.detailId = task.id;
            setPage('agenda');
          },
        }),
      );
    }
    grid.appendChild(cell);
  }
  dom.calendar.appendChild(grid);
}

function stepMonth(delta) {
  const [year, month] = state.month;
  const date = new Date(year, month - 1 + delta, 1);
  state.month = [date.getFullYear(), date.getMonth() + 1];
  renderCalendar();
}

// ---------- actions ----------

async function toggleComplete(task) {
  const command = task.completed_at ? 'uncomplete_task' : 'complete_task';
  const result = await call(command, { id: task.id }, 'Updating task');
  if (result && !task.completed_at) {
    showToast(
      result.recurrence_label
        ? `Done — next ${formatDue(result.next, state.todayDate).toLowerCase()}`
        : 'Completed  ·  U to undo',
    );
  }
  refresh();
}

async function remove(task) {
  await call('delete_task', { id: task.id }, 'Deleting task');
  if (state.detailId === task.id) state.detailId = null;
  showToast('Deleted  ·  U to undo');
  refresh();
}

async function edit(id, changes) {
  await call('update_task', { edit: { id, ...changes } }, 'Saving task');
  refresh();
}

async function undo() {
  const result = await call('undo', undefined, 'Undo');
  if (!result) {
    showToast('Nothing to undo');
    return;
  }
  showToast(result.title ? `Undid ${result.label}: “${result.title}”` : `Undid ${result.label}`);
  refresh();
}

function showToast(message) {
  dom.toast.textContent = message;
  dom.toast.hidden = false;
  clearTimeout(dom.toast._timer);
  dom.toast._timer = setTimeout(() => {
    dom.toast.hidden = true;
  }, 3200);
}

// ---------- pages ----------

function setPage(page) {
  state.page = page;
  for (const name of PAGES) {
    dom[`page-${name}`].classList.toggle('active', name === page);
    dom[`${name}-page`].hidden = name !== page;
  }

  // Capture is for tasks; the habit month has nothing to capture into.
  dom.capture.hidden = page === 'habits';
  // The sidebar, search and detail pane only mean anything on the task list.
  const tasks = page === 'agenda';
  document.querySelector('.sidebar').hidden = !tasks;
  document.querySelector('.page-tools').hidden = !tasks;
  if (!tasks) dom.detail.hidden = true;

  refresh();
}

// ---------- keyboard ----------

/**
 * Whether the focused element should get the keystroke instead of the global
 * shortcuts.
 *
 * Fields are the obvious case, but buttons matter just as much: a single-key
 * shortcut that eats Enter stops every button in the app from being reachable
 * by keyboard, which in a keyboard-first app is a defect rather than a detail.
 * The task list is the deliberate exception — it is focusable precisely so the
 * shortcuts work while it has focus.
 */
function takesKeys(target) {
  if (!(target instanceof Element) || target === dom.agenda) return false;
  return (
    target.matches('input, textarea, select, button, a[href], [contenteditable="true"]') ||
    target.closest('.detail, .popover, .journal-entries, .habit-grid') !== null
  );
}

function moveCursor(delta) {
  if (state.page !== 'agenda' || !state.rows.length) return;
  state.cursor = Math.min(Math.max(state.cursor + delta, 0), state.rows.length - 1);
  state.detailId = state.rows[state.cursor].id;
  renderAgenda();
  renderPane();
  dom.agenda.querySelectorAll('.task')[state.cursor]?.scrollIntoView({ block: 'nearest' });
}

document.addEventListener('keydown', (event) => {
  const typing = takesKeys(event.target);

  if (event.key === 'Escape') {
    if (typing) {
      event.target.blur();
      if (event.target === dom.search) {
        state.search = '';
        dom.search.value = '';
        refresh();
      }
    } else if (state.detailId) {
      closeDetail();
    }
    return;
  }

  if (event.ctrlKey && event.key.toLowerCase() === 'z' && !typing) {
    event.preventDefault();
    undo();
    return;
  }
  if (typing) return;

  switch (event.key.toLowerCase()) {
    case 'n':
      event.preventDefault();
      if (state.page === 'habits') setPage('agenda');
      capture.focus();
      break;
    case '/':
      event.preventDefault();
      setPage('agenda');
      dom.search.focus();
      break;
    case 'j':
    case 'arrowdown':
      event.preventDefault();
      moveCursor(1);
      break;
    case 'k':
    case 'arrowup':
      event.preventDefault();
      moveCursor(-1);
      break;
    case ' ':
    case 'x':
      event.preventDefault();
      if (state.rows[state.cursor]) toggleComplete(state.rows[state.cursor]);
      break;
    case 'enter':
      event.preventDefault();
      if (state.rows[state.cursor]) openDetail(state.rows[state.cursor].id);
      break;
    case 'e':
      event.preventDefault();
      if (state.rows[state.cursor]) openDetail(state.rows[state.cursor].id);
      dom.detail.querySelector('.detail-title')?.focus();
      break;
    case 'delete':
    case 'backspace':
      event.preventDefault();
      if (state.rows[state.cursor]) remove(state.rows[state.cursor]);
      break;
    case 'u':
      event.preventDefault();
      undo();
      break;
    case 'w':
      event.preventDefault();
      call('toggle_widget_command', undefined, 'Sticky note');
      break;
    case '1':
    case '2':
    case '3':
      event.preventDefault();
      setPage(PAGES[Number(event.key) - 1]);
      break;
    default:
      break;
  }
});

// ---------- wiring ----------

dom['add-button'].addEventListener('click', () => capture.submit());

for (const name of PAGES) {
  dom[`page-${name}`].addEventListener('click', () => setPage(name));
}

dom['month-prev'].addEventListener('click', () => habitMonth.step(-1));
dom['month-next'].addEventListener('click', () => habitMonth.step(1));
dom['month-today'].addEventListener('click', () => habitMonth.thisMonth());

let searchTimer = null;
dom.search.addEventListener('input', () => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    state.search = dom.search.value;
    state.cursor = 0;
    refresh();
  }, 140);
});

dom['show-completed'].addEventListener('change', () => {
  state.showCompleted = dom['show-completed'].checked;
  state.cursor = 0;
  refresh();
});

// Any window can change the data; all of them re-read when it happens.
listen('data-changed', () => refresh());

async function boot() {
  await refresh();
  capture.focus();
}

boot();
