// Master Todo — main window.
//
// Keyboard-first by construction: every action has a key, the list owns focus
// by default, and the capture field is one keystroke away from anywhere.

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

const VIEWS = [
  { id: 'today', label: 'Today', filter: 'Today', key: 't' },
  { id: 'next7', label: 'Next 7 days', filter: { Next: { days: 7 } }, key: 'n' },
  { id: 'all', label: 'All', filter: 'All', key: 'a' },
  { id: 'overdue', label: 'Overdue', filter: 'Overdue', key: 'o' },
  { id: 'inbox', label: 'Inbox', filter: 'Inbox', key: 'i' },
  { id: 'done', label: 'Completed', filter: 'Completed', key: 'd' },
];

const state = {
  view: VIEWS[0],
  /** Overrides the sidebar selection while the search box has text. */
  search: '',
  tasks: [],
  cursor: 0,
  layout: 'list',
  detailId: null,
  todayDate: new Date(),
  month: null,
};

const dom = {
  quickAdd: document.getElementById('quick-add'),
  addButton: document.getElementById('add-button'),
  preview: document.getElementById('preview'),
  views: document.getElementById('views'),
  projects: document.getElementById('projects'),
  tags: document.getElementById('tags'),
  title: document.getElementById('view-title'),
  search: document.getElementById('search'),
  list: document.getElementById('task-list'),
  calendar: document.getElementById('calendar'),
  empty: document.getElementById('empty'),
  detail: document.getElementById('detail'),
  summary: document.getElementById('summary'),
  toast: document.getElementById('toast'),
  tabList: document.getElementById('tab-list'),
  tabCalendar: document.getElementById('tab-calendar'),
};

// ---------- data ----------

/** The filter actually in force: a search box with text overrides the view. */
function activeFilter() {
  return state.search.trim() ? { Search: state.search.trim() } : state.view.filter;
}

async function refresh() {
  state.todayDate = (await today()) ?? new Date();
  const tasks = await call('list_tasks', { filter: activeFilter(), sort: 'Due' }, 'Loading tasks');
  state.tasks = tasks ?? [];

  // Projects, tags and counts all move whenever a task does, so they are read
  // here rather than once at startup.
  state.projects = (await call('projects', undefined, 'Projects')) ?? [];
  state.tags = (await call('tags', undefined, 'Tags')) ?? [];
  state.counts = (await call('view_counts', undefined, 'Counts')) ?? {};
  if (state.cursor >= state.tasks.length) state.cursor = Math.max(0, state.tasks.length - 1);

  renderList();
  renderSidebar();
  renderDetail();
  if (state.layout === 'calendar') renderCalendar();

  const summary = await call('summary', undefined, 'Summary');
  dom.summary.textContent = summary ?? '';
}

// ---------- rendering ----------

function renderSidebar() {
  clear(dom.views);
  for (const view of VIEWS) {
    const selected = !state.search && view.id === state.view.id;
    const count = state.counts?.[view.id];
    dom.views.appendChild(
      el('li', { class: selected ? 'selected' : '', onclick: () => selectView(view) }, [
        el('span', { text: view.label }),
        count ? el('span', { class: 'count', text: String(count) }) : null,
      ]),
    );
  }
  renderNamedList(dom.projects, state.projects ?? [], (name) => ({ Project: name }), '');
  renderNamedList(dom.tags, state.tags ?? [], (name) => ({ Tag: name }), '#');
}

function renderNamedList(node, names, toFilter, prefix) {
  clear(node);
  for (const name of names) {
    const filter = toFilter(name);
    const selected = JSON.stringify(state.view.filter) === JSON.stringify(filter);
    node.appendChild(
      el('li', {
        class: selected ? 'selected' : '',
        text: prefix + name,
        onclick: () => selectView({ id: prefix + name, label: prefix + name, filter }),
      }),
    );
  }
}

function renderList() {
  dom.title.textContent = state.search ? `Search: ${state.search}` : state.view.label;
  dom.list.hidden = state.layout !== 'list';
  dom.calendar.hidden = state.layout !== 'calendar';

  clear(dom.list);
  if (state.layout !== 'list') {
    dom.empty.hidden = true;
    return;
  }

  if (!state.tasks.length) {
    dom.empty.hidden = false;
    dom.empty.textContent = state.search
      ? 'Nothing matches that search.'
      : 'Nothing here. Press N to add something.';
    return;
  }
  dom.empty.hidden = true;

  state.tasks.forEach((task, index) => {
    dom.list.appendChild(taskRow(task, index));
  });
}

function taskRow(task, index) {
  const done = Boolean(task.completed_at);
  const classes = ['task', priorityClass(task.priority), done ? 'done' : '', index === state.cursor ? 'active' : ''];

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
  if (dueIso) {
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
        state.detailId = task.id;
        renderList();
        renderDetail();
      },
    },
    [check, main],
  );
}

function renderDetail() {
  const task = state.tasks.find((t) => t.id === state.detailId);
  if (!task) {
    dom.detail.hidden = true;
    return;
  }
  dom.detail.hidden = false;
  clear(dom.detail);

  const titleInput = el('input', { type: 'text', value: task.title });
  titleInput.addEventListener('change', () =>
    edit(task.id, { title: titleInput.value }),
  );

  const notes = el('textarea', {}, task.notes ?? '');
  notes.value = task.notes ?? '';
  notes.addEventListener('change', () => edit(task.id, { notes: notes.value }));

  const due = el('input', { type: 'date', value: task.due ?? '' });
  due.addEventListener('change', () => edit(task.id, { due: [due.value || null] }));

  const time = el('input', { type: 'time', value: shortTime(task.time) });
  time.addEventListener('change', () => edit(task.id, { time: [time.value || null] }));

  const priority = el('select');
  for (const [value, label] of [
    ['none', 'None'],
    ['high', 'High'],
    ['medium', 'Medium'],
    ['low', 'Low'],
  ]) {
    const option = el('option', { value, text: label });
    if (String(task.priority).toLowerCase() === value) option.selected = true;
    priority.appendChild(option);
  }
  priority.addEventListener('change', () => edit(task.id, { priority: priority.value }));

  const project = el('input', { type: 'text', value: task.project ?? '', placeholder: 'none' });
  project.addEventListener('change', () => edit(task.id, { project: [project.value || null] }));

  const tags = el('input', {
    type: 'text',
    value: (task.tags ?? []).join(', '),
    placeholder: 'comma separated',
  });
  tags.addEventListener('change', () =>
    edit(task.id, { tags: tags.value.split(',').map((t) => t.trim()).filter(Boolean) }),
  );

  const subtasks = el('ul', { class: 'subtasks' });
  for (const sub of task.subtasks ?? []) {
    const box = el('input', { type: 'checkbox' });
    box.checked = sub.done;
    box.addEventListener('change', () =>
      call('toggle_subtask', { id: task.id, subtaskId: sub.id }, 'Subtask').then(refresh),
    );
    subtasks.appendChild(
      el('li', { class: sub.done ? 'done' : '' }, [box, el('span', { text: sub.title })]),
    );
  }
  const newSub = el('input', { type: 'text', placeholder: 'Add a subtask…' });
  newSub.addEventListener('keydown', async (event) => {
    if (event.key !== 'Enter' || !newSub.value.trim()) return;
    await call('add_subtask', { id: task.id, title: newSub.value.trim() }, 'Subtask');
    newSub.value = '';
    refresh();
  });

  const actions = [
    el('button', {
      type: 'button',
      text: task.completed_at ? 'Reopen' : 'Complete',
      onclick: () => toggleComplete(task),
    }),
  ];
  if (task.recurrence_label) {
    const occurrence = task.next ?? task.due;
    actions.push(
      el('button', {
        type: 'button',
        text: 'Skip this one',
        title: 'Skip this occurrence, keep the series',
        onclick: async () => {
          await call('skip_occurrence', { id: task.id, date: occurrence }, 'Skip');
          showToast('Occurrence skipped');
          refresh();
        },
      }),
      el('button', {
        type: 'button',
        text: 'Push a day',
        title: 'Move just this occurrence to tomorrow',
        onclick: async () => {
          const next = parseDate(occurrence);
          next.setDate(next.getDate() + 1);
          await call(
            'reschedule_occurrence',
            { id: task.id, date: occurrence, to: toIso(next) },
            'Reschedule',
          );
          showToast('Occurrence moved to the next day');
          refresh();
        },
      }),
    );
  }
  actions.push(
    el('button', {
      type: 'button',
      class: 'danger',
      text: 'Delete',
      onclick: () => remove(task),
    }),
  );

  dom.detail.append(
    el('h2', { text: 'Task' }),
    field('Title', titleInput),
    field('Due', due),
    field('Time', time),
    field('Priority', priority),
    field('Project', project),
    field('Tags', tags),
    field('Notes', notes),
    field('Subtasks', el('div', {}, [subtasks, newSub])),
    el('div', { class: 'detail-actions' }, actions),
  );
}

function field(label, control) {
  return el('div', { class: 'field' }, [el('label', { text: label }), control]);
}

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
      el('button', { type: 'button', text: 'Today', onclick: () => { state.month = null; renderCalendar(); } }),
    ]),
  );

  const grid = el('div', { class: 'cal-grid' });
  const dow = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
  for (const name of dow) grid.appendChild(el('div', { class: 'cal-dow', text: name }));

  const first = new Date(year, month - 1, 1);
  // Monday-first grid: JS weeks start on Sunday, so shift by one.
  const lead = (first.getDay() + 6) % 7;
  const start = new Date(year, month - 1, 1 - lead);
  const todayIso = toIso(state.todayDate);

  for (let i = 0; i < 42; i += 1) {
    const date = new Date(start);
    date.setDate(start.getDate() + i);
    const iso = toIso(date);
    const inMonth = date.getMonth() === month - 1;
    const cell = el('div', {
      class: ['cal-day', inMonth ? '' : 'other', iso === todayIso ? 'today' : ''].filter(Boolean).join(' '),
    });
    cell.appendChild(el('div', { class: 'cal-date', text: String(date.getDate()) }));
    for (const task of byDate.get(iso) ?? []) {
      cell.appendChild(
        el('div', {
          class: 'cal-task',
          text: task.title,
          title: task.title,
          onclick: () => {
            state.detailId = task.id;
            renderDetail();
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

function selectView(view) {
  state.view = view;
  state.search = '';
  dom.search.value = '';
  state.cursor = 0;
  refresh();
}

async function add() {
  const line = dom.quickAdd.value.trim();
  if (!line) return;
  const task = await call('quick_add', { line }, 'Adding task');
  if (!task) return;
  dom.quickAdd.value = '';
  renderPreview(null);
  showToast(`Added “${task.title}”`);
  refresh();
}

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

// ---------- live parse preview ----------

let previewTimer = null;

function renderPreview(preview) {
  clear(dom.preview);
  if (!preview) return;

  const chips = [];
  if (preview.title) chips.push(['accent', preview.title]);
  if (preview.due) chips.push(['', formatDue(preview.due, state.todayDate)]);
  if (preview.time) chips.push(['', formatTime(preview.time)]);
  if (preview.recurrence_label) chips.push(['', `↻ ${preview.recurrence_label}`]);
  if (preview.priority && preview.priority !== 'none') chips.push(['', `! ${preview.priority}`]);
  if (preview.project) chips.push(['', `@${preview.project}`]);
  for (const tag of preview.tags ?? []) chips.push(['', `#${tag}`]);

  for (const [variant, text] of chips) {
    dom.preview.appendChild(el('span', { class: `chip ${variant}`.trim(), text }));
  }
  for (const guess of preview.guesses ?? []) {
    dom.preview.appendChild(el('span', { class: 'chip guess', text: guess }));
  }
}

function schedulePreview() {
  clearTimeout(previewTimer);
  const line = dom.quickAdd.value;
  if (!line.trim()) {
    renderPreview(null);
    return;
  }
  // Debounced: the preview should feel live without a round trip per keystroke.
  previewTimer = setTimeout(async () => {
    renderPreview(await call('preview_line', { line }, 'Preview'));
  }, 90);
}

// ---------- keyboard ----------

function isTyping(target) {
  return target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement;
}

function moveCursor(delta) {
  if (!state.tasks.length) return;
  state.cursor = Math.min(Math.max(state.cursor + delta, 0), state.tasks.length - 1);
  state.detailId = state.tasks[state.cursor].id;
  renderList();
  renderDetail();
  dom.list.children[state.cursor]?.scrollIntoView({ block: 'nearest' });
}

document.addEventListener('keydown', (event) => {
  const typing = isTyping(event.target);

  if (event.key === 'Escape') {
    if (typing) {
      event.target.blur();
      if (event.target === dom.search) {
        state.search = '';
        dom.search.value = '';
        refresh();
      }
    } else if (state.detailId) {
      state.detailId = null;
      renderDetail();
    }
    return;
  }

  // Ctrl+Z works wherever the user is, as it does everywhere else in Windows.
  if (event.ctrlKey && event.key.toLowerCase() === 'z' && !typing) {
    event.preventDefault();
    undo();
    return;
  }

  if (typing) {
    if (event.target === dom.quickAdd && event.key === 'Enter') {
      event.preventDefault();
      add();
    }
    return;
  }

  switch (event.key.toLowerCase()) {
    case 'n':
      event.preventDefault();
      dom.quickAdd.focus();
      break;
    case '/':
      event.preventDefault();
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
      if (state.tasks[state.cursor]) toggleComplete(state.tasks[state.cursor]);
      break;
    case 'enter':
      event.preventDefault();
      state.detailId = state.tasks[state.cursor]?.id ?? null;
      renderDetail();
      break;
    case 'e':
      event.preventDefault();
      state.detailId = state.tasks[state.cursor]?.id ?? null;
      renderDetail();
      dom.detail.querySelector('input')?.focus();
      break;
    case 'delete':
    case 'backspace':
      event.preventDefault();
      if (state.tasks[state.cursor]) remove(state.tasks[state.cursor]);
      break;
    case 'u':
      event.preventDefault();
      undo();
      break;
    case 'c':
      event.preventDefault();
      setLayout(state.layout === 'list' ? 'calendar' : 'list');
      break;
    case 'w':
      // The widget is otherwise only reachable from the tray menu.
      event.preventDefault();
      call('toggle_widget_command', undefined, 'Widget');
      break;
    default: {
      // Number keys jump straight to a view.
      const index = Number(event.key) - 1;
      if (Number.isInteger(index) && VIEWS[index]) {
        event.preventDefault();
        selectView(VIEWS[index]);
      }
    }
  }
});

function setLayout(layout) {
  state.layout = layout;
  dom.tabList.classList.toggle('active', layout === 'list');
  dom.tabCalendar.classList.toggle('active', layout === 'calendar');
  renderList();
  if (layout === 'calendar') renderCalendar();
}

// ---------- wiring ----------

dom.addButton.addEventListener('click', add);
dom.quickAdd.addEventListener('input', schedulePreview);
dom.tabList.addEventListener('click', () => setLayout('list'));
dom.tabCalendar.addEventListener('click', () => setLayout('calendar'));

let searchTimer = null;
dom.search.addEventListener('input', () => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    state.search = dom.search.value;
    state.cursor = 0;
    refresh();
  }, 140);
});

// Any window can change the data; all of them re-read when it happens.
listen('tasks-changed', () => refresh());

async function boot() {
  await refresh();
  dom.quickAdd.focus();
}

boot();
