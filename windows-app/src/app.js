// Kairos — main window, Direction B.
//
// A rail for the three pages, a mono status line that says where you are and
// what you can press, the page itself, and a capture bar pinned to the bottom
// that is always there and never a modal.

import { createCapture } from './capture.js';
import { refreshHabits, renderDetail } from './detail.js';
import { createHabits } from './habits.js';
import { createPrayer } from './prayer.js';
import { createSettings } from './settings.js';
import { createWorkout } from './workout.js';
import { call, clear, el, listen, parseDate, toIso, today } from './shared.js';

const PAGES = ['agenda', 'calendar', 'habits', 'workout', 'prayer', 'settings'];

/** Pages a setting can switch off. Tasks and Settings are always reachable. */
const OPTIONAL_PAGES = {
  calendar: 'calendar_page',
  habits: 'habits_page',
  workout: 'workout_page',
  prayer: 'prayer_page',
};

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
  todayDate: new Date(),
  month: null,
  projects: [],
  tags: [],
  // What the core says is turned on. Read once at boot, then whenever the
  // settings page changes something.
  settings: {},
};

const dom = {};
for (const id of [
  'error-bar', 'here', 'count', 'status-tools', 'agenda', 'empty', 'agenda-page',
  'calendar-page', 'cal-dow', 'cal-grid', 'habits-page', 'habit-rows', 'journal',
  'summary', 'detail', 'toast', 'capture', 'capture-hint', 'preview', 'quick-add',
  'highlight', 'page-agenda', 'page-calendar', 'page-habits', 'page-workout',
  'sticky-toggle', 'lists', 'routines', 'workout-page', 'workout-grid', 'workout-empty',
  'page-settings', 'settings-page', 'settings', 'page-prayer', 'prayer-page',
]) {
  dom[id] = document.getElementById(id);
}

// ---------- capture ----------

const settings = createSettings({
  container: dom.settings,
  onChange: loadSettings,
});

const prayer = createPrayer({
  page: dom['prayer-page'],
  onStatus: (label, method) => {
    dom.here.textContent = label;
    dom.count.textContent = method;
    clear(dom['status-tools']);
  },
});

const capture = createCapture({
  input: dom['quick-add'],
  layer: dom.highlight,
  previewNode: dom.preview,
  getToday: () => state.todayDate,
  onAdd: (task) => {
    toast(`added ${task.title}`);
    refresh();
  },
  // On the habits page the same bar writes the day's journal line, or adds a
  // habit when the line is prefixed with "+".
  onJournal: async (line) => {
    if (line.startsWith('+')) {
      await habits.addHabit(line.slice(1).trim());
      toast('habit added');
      return;
    }
    await habits.submit(line);
    toast('noted');
  },
});

// ---------- habits ----------

const habits = createHabits({
  rows: dom['habit-rows'],
  journal: dom.journal,
  summary: dom.summary,
  getToday: () => state.todayDate,
  onMonth: (label, count) => {
    if (state.page !== 'habits') return;
    dom.here.textContent = label;
    dom.count.textContent = `${count} habit${count === 1 ? '' : 's'}`;
  },
});

const workout = createWorkout({
  routinesNode: dom.routines,
  gridNode: dom['workout-grid'],
  emptyNode: dom['workout-empty'],
  onRoutine: (name, sessions) => {
    if (state.page !== 'workout') return;
    dom.here.textContent = name;
    dom.count.textContent = `${sessions} session${sessions === 1 ? '' : 's'}`;
  },
});

// ---------- data ----------

async function refresh() {
  state.todayDate = (await today()) ?? new Date();

  if (state.page === 'settings') {
    await settings.load();
    renderStatus();
    return;
  }

  if (state.page === 'prayer') {
    await prayer.load();
    return;
  }

  // The detail pane's habit picker names them, and a habit added on the
  // habits page has to show up there without a restart.
  await refreshHabits();

  if (state.page === 'habits') {
    await habits.load();
  } else if (state.page === 'workout') {
    await workout.load();
    renderWorkoutTools();
    return;
  } else {
    await loadAgenda();
    if (state.page === 'calendar') await renderCalendar();
  }

  state.projects = (await call('projects', undefined, 'Projects')) ?? [];
  state.tags = (await call('tags', undefined, 'Tags')) ?? [];
  renderStatus();
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

// ---------- the status line ----------

function renderStatus() {
  // These pages own their own line.
  if (state.page === 'habits' || state.page === 'workout' || state.page === 'prayer') return;

  if (state.page === 'settings') {
    dom.here.textContent = TITLES.settings;
    dom.count.textContent = '';
    clear(dom['status-tools']);
    return;
  }

  const open = state.rows.filter((t) => !t.completed_at).length;
  if (state.page === 'calendar') {
    const [year, month] = state.month ?? [];
    dom.here.textContent = year
      ? new Date(year, month - 1, 1).toLocaleDateString(undefined, {
          month: 'long',
          year: 'numeric',
        })
      : '';
    dom.count.textContent = '';
    renderCalendarTools();
    return;
  }
  dom.here.textContent = state.search
    ? `search: ${state.search}`
    : state.project
      ? state.project
      : state.tag
        ? `#${state.tag}`
        : 'Tasks';
  dom.count.textContent = `${open} open`;
  clear(dom['status-tools']);

  // Search is a field that looks like the rest of the line until you use it.
  const search = el('input', {
    class: 'search',
    type: 'search',
    placeholder: '/ search',
    value: state.search,
    'aria-label': 'Search tasks',
  });
  let timer = null;
  search.addEventListener('input', () => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      state.search = search.value;
      state.cursor = 0;
      refresh();
    }, 140);
  });
  dom['status-tools'].appendChild(search);

  dom['status-tools'].appendChild(
    el('button', {
      type: 'button',
      class: state.showCompleted ? 'on' : '',
      text: 'done',
      title: 'Show completed',
      onclick: () => {
        state.showCompleted = !state.showCompleted;
        state.cursor = 0;
        refresh();
      },
    }),
  );
  renderLists();
}

function renderWorkoutTools() {
  clear(dom['status-tools']);
  dom['status-tools'].appendChild(
    el('button', {
      type: 'button',
      class: 'on',
      text: 'S start session',
      title: "Starts today's session with last time's numbers",
      onclick: startSession,
    }),
  );
}

/** The second sidebar: the lists a task can belong to, and the tags in use. */
function renderLists() {
  clear(dom.lists);

  dom.lists.appendChild(el('div', { class: 'lists-head', text: 'LISTS' }));
  dom.lists.appendChild(
    el('button', {
      type: 'button',
      class: `list-row ${!state.project && !state.tag ? 'on' : ''}`.trim(),
      onclick: () => {
        state.project = null;
        state.tag = null;
        state.cursor = 0;
        refresh();
      },
    }, [el('span', { class: 'name', text: 'All tasks' })]),
  );

  for (const name of state.projects) {
    dom.lists.appendChild(
      el('button', {
        type: 'button',
        class: `list-row ${state.project === name ? 'on' : ''}`.trim(),
        // Clicking the active one clears it, so a list is never a trap.
        onclick: () => {
          state.project = state.project === name ? null : name;
          state.tag = null;
          state.cursor = 0;
          refresh();
        },
      }, [el('span', { class: 'name', text: name })]),
    );
  }

  if (state.tags.length) {
    dom.lists.appendChild(el('div', { class: 'lists-head', text: 'TAGS' }));
    for (const name of state.tags) {
      dom.lists.appendChild(
        el('button', {
          type: 'button',
          class: `list-row ${state.tag === name ? 'on' : ''}`.trim(),
          onclick: () => {
            state.tag = state.tag === name ? null : name;
            state.project = null;
            state.cursor = 0;
            refresh();
          },
        }, [el('span', { class: 'name', text: `#${name}` })]),
      );
    }
  }
}

function renderCalendarTools() {
  clear(dom['status-tools']);
  dom['status-tools'].append(
    el('button', { type: 'button', text: '‹ H', onclick: () => stepMonth(-1) }),
    el('button', {
      class: 'on',
      type: 'button',
      text: 'T today',
      onclick: () => {
        state.month = null;
        renderCalendar();
      },
    }),
    el('button', { type: 'button', text: 'L ›', onclick: () => stepMonth(1) }),
  );
}

// ---------- the task list ----------

function renderAgenda() {
  clear(dom.agenda);

  if (!state.rows.length) {
    dom.empty.hidden = false;
    dom.empty.textContent = state.search
      ? 'nothing matches'
      : 'nothing ahead — press N';
    return;
  }
  dom.empty.hidden = true;

  // One flat index across the groups, so J/K walks the whole list.
  let index = 0;
  for (const group of state.groups) {
    dom.agenda.appendChild(
      el('li', { class: `group ${group.id}` }, [
        el('span', { text: group.label.toUpperCase() }),
        el('span', { class: 'rule' }),
        el('span', { class: 'count', text: String(group.tasks.length) }),
      ]),
    );
    for (const task of group.tasks) {
      dom.agenda.appendChild(taskRow(task, index, group.id));
      index += 1;
    }
  }
}

/** The meta column: one mono line, dot separated, cut rather than wrapped. */
function metaFor(task, groupId) {
  const bits = [];
  if (task.overdue) {
    const days = Math.round((parseDate(task.next ?? task.due) - state.todayDate) / 86400000);
    bits.push(`${days}d`);
  } else if (task.time) {
    bits.push(task.time.slice(0, 5));
  } else if (!['today', 'tomorrow'].includes(groupId) && (task.next ?? task.due)) {
    const date = parseDate(task.next ?? task.due);
    bits.push(date.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric' }));
  }
  // p1/p2/p3 reads in a mono column where "medium" does not.
  const rank = { High: 'p1', Medium: 'p2', Low: 'p3' }[task.priority];
  if (rank) bits.unshift(rank);
  if (task.recurrence_label) bits.push(shortRule(task.recurrence_label));
  if (task.project) bits.push(`@${task.project}`);
  for (const tag of task.tags ?? []) bits.push(`#${tag}`);
  if (task.subtasks?.length) bits.push(`${task.subtasks_done}/${task.subtasks.length}`);
  return bits;
}

/** "every weekday" reads as "weekdays" in a column this narrow. */
function shortRule(label) {
  return label
    .replace(/^every weekday$/, 'weekdays')
    .replace(/^every day$/, 'daily')
    .replace(/^every /, '');
}

function taskRow(task, index, groupId) {
  const done = Boolean(task.completed_at);
  const classes = ['task'];
  if (task.overdue) classes.push('late');
  else if (groupId === 'today') classes.push('now');
  if (done) classes.push('done');
  if (index === state.cursor) classes.push('active');

  const check = el('button', {
    class: `check ${done ? 'on' : ''}`.trim(),
    type: 'button',
    'aria-label': `${done ? 'Reopen' : 'Complete'}: ${task.title}`,
    onclick: (event) => {
      event.stopPropagation();
      toggleComplete(task);
    },
  });

  const bits = metaFor(task, groupId);
  const meta = el('span', { class: 'meta' });
  bits.forEach((bit, i) => {
    if (i) meta.append(' · ');
    // Everything past the first two drops out in a narrow window.
    meta.appendChild(el('span', { class: i > 1 ? 'extra' : '', text: bit }));
  });

  return el(
    'li',
    {
      class: classes.join(' '),
      onclick: () => {
        state.cursor = index;
        openDetail(task.id);
      },
    },
    [check, el('span', { class: 'title', text: task.title }), meta],
  );
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

  renderDetail(dom.detail, task, {
    todayDate: state.todayDate,
    actions: {
      close: closeDetail,
      edit: (changes) => edit(task.id, changes),
      toggleComplete,
      remove,
      skip: async (t, date) => {
        await call('skip_occurrence', { id: t.id, date }, 'Skip');
        toast('Occurrence skipped');
        refresh();
      },
      push: async (t, date) => {
        const next = parseDate(date);
        next.setDate(next.getDate() + 1);
        await call('reschedule_occurrence', { id: t.id, date, to: toIso(next) }, 'Reschedule');
        toast('Moved to the next day');
        refresh();
      },
      toggleSubtask: async (t, sub) => {
        await call('toggle_subtask', { id: t.id, subtaskId: sub.id }, 'Subtask');
        refresh();
      },
      addSubtask: async (t, title) => {
        await call('add_subtask', { id: t.id, title }, 'Subtask');
        await refresh();
        // The pane is rebuilt on refresh, so focus has to be put back or the
        // next item typed lands in whatever the global shortcuts do with it.
        dom.detail.querySelector('.add-row input')?.focus();
      },
    },
  });
}

// ---------- calendar ----------

async function renderCalendar() {
  const [year, month] = state.month ?? (await call('current_month', undefined, 'Calendar')) ?? [];
  if (!year) return;
  state.month = [year, month];

  dom.here.textContent = new Date(year, month - 1, 1).toLocaleDateString(undefined, {
    month: 'long',
    year: 'numeric',
  });

  const days = (await call('calendar_month', { year, month }, 'Calendar')) ?? [];
  const byDate = new Map(days.map((d) => [d.date, d.tasks]));

  const DOW = ['SUN', 'MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT'];
  const mondayFirst = state.settings.week_starts_monday !== false;

  clear(dom['cal-dow']);
  for (const index of mondayFirst ? [1, 2, 3, 4, 5, 6, 0] : [0, 1, 2, 3, 4, 5, 6]) {
    dom['cal-dow'].appendChild(el('span', { text: DOW[index] }));
  }

  clear(dom['cal-grid']);
  const first = new Date(year, month - 1, 1);
  const lead = mondayFirst ? (first.getDay() + 6) % 7 : first.getDay();
  const start = new Date(year, month - 1, 1 - lead);
  const todayIso = toIso(state.todayDate);

  // Enough whole weeks to hold the month: a 31-day month that starts on the
  // last day of the week needs six rows, and five would silently cut it off.
  const length = new Date(year, month, 0).getDate();
  const cells = Math.ceil((lead + length) / 7) * 7;

  for (let i = 0; i < cells; i += 1) {
    const date = new Date(start);
    date.setDate(start.getDate() + i);
    const iso = toIso(date);
    const inMonth = date.getMonth() === month - 1;
    const items = byDate.get(iso) ?? [];
    const overdue = items.some((t) => t.overdue);

    const classes = ['cal-day'];
    if (!inMonth) classes.push('other');
    else if (iso === todayIso) classes.push('today');
    else if (overdue) classes.push('late');

    const cell = el('div', { class: classes.join(' ') });
    if (!inMonth) {
      dom['cal-grid'].appendChild(cell);
      continue;
    }

    if (iso === todayIso) {
      cell.appendChild(
        el('div', { class: 'head' }, [
          el('span', { class: 'num', text: String(date.getDate()) }),
          items.length ? el('span', { class: 'n', text: String(items.length) }) : null,
        ]),
      );
    } else {
      cell.appendChild(el('span', { class: 'num', text: String(date.getDate()) }));
    }

    // Two fit; the rest become a count, which is more honest than a clipped row.
    for (const task of items.slice(0, 2)) {
      cell.appendChild(
        el('span', {
          class: 'item',
          text: task.time ? `${task.time.slice(0, 5)} ${task.title}` : task.title,
          title: task.title,
          onclick: () => {
            state.detailId = task.id;
            setPage('agenda');
          },
        }),
      );
    }
    if (items.length > 2) {
      cell.appendChild(el('span', { class: 'more', text: `+${items.length - 2}` }));
    }
    dom['cal-grid'].appendChild(cell);
  }
}

function stepMonth(delta) {
  const [year, month] = state.month ?? [state.todayDate.getFullYear(), state.todayDate.getMonth() + 1];
  const date = new Date(year, month - 1 + delta, 1);
  state.month = [date.getFullYear(), date.getMonth() + 1];
  renderCalendar();
}

// ---------- actions ----------

async function toggleComplete(task) {
  const command = task.completed_at ? 'uncomplete_task' : 'complete_task';
  const result = await call(command, { id: task.id }, 'Updating task');
  if (result && !task.completed_at) {
    toast(
      result.recurrence_label && result.next
        ? `${task.title} done — next ${parseDate(result.next).toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short' })}`
        : `${task.title} done`,
      true,
    );
  }
  refresh();
}

async function remove(task) {
  if (state.settings.confirm_delete && !window.confirm(`Delete "${task.title}"?`)) return;
  await call('delete_task', { id: task.id }, 'Deleting task');
  if (state.detailId === task.id) state.detailId = null;
  toast(`${task.title} deleted`, true);
  refresh();
}

async function edit(id, changes) {
  await call('update_task', { edit: { id, ...changes } }, 'Saving task');
  refresh();
}

async function undo() {
  const result = await call('undo', undefined, 'Undo');
  if (!result) {
    toast('nothing to undo');
    return;
  }
  toast(result.title ? `undid ${result.label}: ${result.title}` : `undid ${result.label}`);
  refresh();
}

async function redo() {
  const result = await call('redo', undefined, 'Redo');
  if (!result) {
    toast('nothing to redo');
    return;
  }
  toast(result.title ? `redid ${result.label}: ${result.title}` : `redid ${result.label}`);
  refresh();
}

async function startSession() {
  if (await workout.startSession()) toast("today's session started from last time");
}

/** Every destructive act leaves one, with the key that reverses it. */
function toast(message, undoable = false) {
  clear(dom.toast);
  dom.toast.appendChild(el('span', { text: message }));
  if (undoable) {
    dom.toast.appendChild(
      el('button', { type: 'button', text: 'U undo', onclick: undo }),
    );
  }
  dom.toast.hidden = false;
  clearTimeout(dom.toast._timer);
  dom.toast._timer = setTimeout(() => {
    dom.toast.hidden = true;
  }, 3600);
}

// ---------- pages ----------

const HINTS = {
  agenda: 'J K move · X done · U undo · Y redo',
  calendar: 'H L month · T today · Enter opens',
  habits: 'H L month · Space toggles today',
  workout: 'S starts today · double-click renames',
  prayer: 'H L day · T today',
  settings: 'every change saves itself',
};

const TITLES = {
  agenda: 'Tasks',
  calendar: 'Calendar',
  habits: 'Habits',
  workout: 'Workout',
  prayer: 'Prayer',
  settings: 'Settings',
};

function setPage(page) {
  // A page that has been switched off is not somewhere to land, including via
  // its number key or a stale state after the switch was flipped.
  const off = OPTIONAL_PAGES[page];
  if (off && state.settings[off] === false) page = 'agenda';

  state.page = page;
  for (const name of PAGES) {
    dom[`page-${name}`].classList.toggle('active', name === page);
    dom[`${name}-page`].hidden = name !== page;
  }

  // No page inherits the last one's status line. Each writes its own as it
  // loads, and a page that writes nothing should show nothing rather than
  // whatever the page before it left there — which is how the workout page's
  // "start session" button ended up sitting above the habit grid.
  clear(dom['status-tools']);
  dom.here.textContent = TITLES[page] ?? '';
  dom.count.textContent = '';
  // The habit editor floats above everything, so leaving the page has to
  // take it with you — a click elsewhere closes it, but a number key does not.
  habits.closeEditor();
  dom.detail.hidden = page !== 'agenda';
  dom['capture-hint'].textContent = HINTS[page];
  // The bar captures a task on two pages and a journal line on the third;
  // there is nothing to capture into on the workout book or in settings.
  const noCapture = page === 'workout' || page === 'settings' || page === 'prayer';
  dom.capture.hidden = noCapture;
  dom.preview.hidden = noCapture;
  dom['quick-add'].placeholder =
    page === 'habits'
      ? `note for ${state.todayDate.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short' })}…   (+name adds a habit)`
      : 'gym every day 5pm @health #fitness !p1';
  capture.setMode(page === 'habits' ? 'journal' : 'task');
  refresh();
}

// ---------- keyboard ----------

function takesKeys(target) {
  if (!(target instanceof Element) || target === dom.agenda) return false;
  return (
    target.matches('input, textarea, select, button, a[href], [contenteditable="true"]') ||
    target.closest('.detail, .pop, .journal, .habit-rows, .workout-grid, .lists') !== null
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
      if (event.target.classList?.contains('search')) {
        state.search = '';
        refresh();
      }
    } else if (state.detailId) {
      closeDetail();
    }
    return;
  }

  if (event.ctrlKey && event.key.toLowerCase() === 'z' && !typing) {
    event.preventDefault();
    if (event.shiftKey) redo();
    else undo();
    return;
  }
  if (event.ctrlKey && event.key.toLowerCase() === 'y' && !typing) {
    event.preventDefault();
    redo();
    return;
  }
  if (typing) return;

  const key = event.key.toLowerCase();

  // H and L step the month on the two pages that have one.
  if ((key === 'h' || key === 'l') && state.page !== 'agenda') {
    event.preventDefault();
    const delta = key === 'h' ? -1 : 1;
    if (state.page === 'calendar') stepMonth(delta);
    else if (state.page === 'prayer') prayer.step(delta);
    else habits.step(delta);
    return;
  }
  if (key === 't' && state.page === 'calendar') {
    event.preventDefault();
    state.month = null;
    renderCalendar();
    return;
  }
  if (key === 't' && state.page === 'prayer') {
    event.preventDefault();
    prayer.today();
    return;
  }

  switch (key) {
    case 'n':
      event.preventDefault();
      capture.focus();
      break;
    case '/':
      event.preventDefault();
      setPage('agenda');
      dom['status-tools'].querySelector('.search')?.focus();
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
      if (state.page === 'habits') habits.toggleToday();
      else if (state.rows[state.cursor]) toggleComplete(state.rows[state.cursor]);
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
    case 'y':
      event.preventDefault();
      redo();
      break;
    case 's':
      if (state.page === 'workout') {
        event.preventDefault();
        startSession();
      }
      break;
    case 'r':
      // Puts back the last phrase taken off with backspace.
      event.preventDefault();
      capture.restore();
      break;
    case 'w':
      event.preventDefault();
      call('toggle_widget_command', undefined, 'Sticky note');
      break;
    case '1':
    case '2':
    case '3':
    case '4':
    case '5':
    case '6':
      event.preventDefault();
      setPage(PAGES[Number(key) - 1]);
      break;
    default:
      break;
  }
});

// ---------- wiring ----------

for (const name of PAGES) {
  dom[`page-${name}`].addEventListener('click', () => setPage(name));
}
dom['sticky-toggle'].addEventListener('click', () =>
  call('toggle_widget_command', undefined, 'Sticky note'),
);

listen('data-changed', () => refresh());
listen('settings-changed', () => loadSettings());

/**
 * Reads the switches and reshapes the window around them.
 *
 * Called at boot and after every change, rather than each switch having its
 * own handler: there is one place that knows what "habits page off" looks
 * like, and it runs the same way whether the change came from this window or
 * from someone editing settings.json in an editor.
 */
function applySettings() {
  const s = state.settings;

  // Theme. "system" means leave it to the media query.
  if (s.theme && s.theme !== 'system') document.documentElement.dataset.theme = s.theme;
  else delete document.documentElement.dataset.theme;

  for (const [page, key] of Object.entries(OPTIONAL_PAGES)) {
    dom[`page-${page}`].hidden = s[key] === false;
  }
  dom['sticky-toggle'].hidden = s.sticky_note === false;
  dom.lists.hidden = s.lists_sidebar === false;
  capture.setPills(s.parse_pills !== false);

  // Standing on a page that has just been switched off.
  const off = OPTIONAL_PAGES[state.page];
  if (off && s[off] === false) setPage('agenda');
}

async function loadSettings() {
  const values = await call('settings_values', undefined, 'Reading settings');
  if (!values) return;
  const first = !Object.keys(state.settings).length;
  state.settings = values;
  // Only at boot: after that, the toggle in the status line is the user's
  // current choice and must not be overwritten under them.
  if (first) state.showCompleted = values.show_completed;
  applySettings();
}

async function boot() {
  await loadSettings();
  await refresh();
  capture.focus();
}

boot();
