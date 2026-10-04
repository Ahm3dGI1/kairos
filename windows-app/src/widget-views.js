import { call, clear, el, today, toIso } from './shared.js';
import { createHabits } from './habits.js';
import { createWorkout } from './workout.js';

export function createWidgetView(context) {
  document.querySelector('.sticky-head .label').textContent = context.view.toUpperCase();
  document.querySelector('.sticky-foot').textContent = 'drag the header to move · layout saves automatically';
  const root = el('main', { class: `widget-content widget-${context.view}` });
  document.querySelector('.sticky-foot').before(root);
  if (context.view === 'calendar') return calendar(root);
  if (context.view === 'habits') return habits(root);
  if (context.view === 'workout') {
    const routines = el('nav', { class: 'widget-routines', 'aria-label': 'Routines' });
    const grid = el('div', { class: 'workout-grid' });
    const empty = el('p', { class: 'empty' });
    root.append(routines, grid, empty);
    const book = createWorkout({ routinesNode: routines, gridNode: grid, emptyNode: empty });
    if (context.routine) book.select(context.routine);
    return book.load;
  }
  root.append(el('p', { class: 'empty', text: 'Settings are available in the main window.' }));
  return async () => {};
}

function controls(label, previous, next, reset) {
  return el('div', { class: 'widget-controls' }, [
    label,
    el('div', { class: 'widget-navigation' }, [
      el('button', { type: 'button', text: '‹', 'aria-label': 'Previous', onclick: previous }),
      reset ? el('button', { type: 'button', text: 'Today', onclick: reset }) : null,
      el('button', { type: 'button', text: '›', 'aria-label': 'Next', onclick: next }),
    ]),
  ]);
}

function habits(root) {
  const label = el('span', { class: 'widget-period' });
  const rows = el('div', { class: 'habit-rows' });
  const summary = el('aside', { class: 'summary widget-habit-stats', 'aria-label': 'Habit statistics' });
  const content = el('div', { class: 'widget-habit-body' }, [rows, summary]);
  const tracker = createHabits({ rows, summary, compact: true, getToday: () => new Date(),
    onMonth: (month, count) => { label.textContent = month; document.getElementById('counts').textContent = `${count} habit${count === 1 ? '' : 's'}`; },
  });
  root.append(controls(label, () => tracker.step(-1), () => tracker.step(1)), content);
  return tracker.load;
}

function calendar(root) {
  let mode = 'month';
  let anchor = null;
  let version = 0;
  const label = el('span', { class: 'widget-period' });
  const grid = el('div', { class: 'widget-calendar-grid' });
  const weekdays = el('div', { class: 'widget-weekdays' });
  const modes = el('div', { class: 'widget-segments', 'aria-label': 'Calendar view' });
  for (const value of ['week', 'month']) {
    modes.append(el('button', { type: 'button', text: value, 'aria-pressed': String(value === mode), onclick: () => {
      mode = value;
      for (const button of modes.children) button.setAttribute('aria-pressed', String(button.textContent === mode));
      load();
    }}));
  }
  const step = delta => {
    if (!anchor) return;
    anchor = mode === 'week' ? new Date(anchor.getFullYear(), anchor.getMonth(), anchor.getDate() + delta * 7)
      : new Date(anchor.getFullYear(), anchor.getMonth() + delta, 1);
    load();
  };
  const toolbar = controls(label, () => step(-1), () => step(1), () => { anchor = null; load(); });
  root.append(modes, toolbar, weekdays, grid);
  async function load() {
    const request = ++version;
    const now = (await today()) ?? new Date();
    anchor ??= now;
    const settings = await call('settings_values');
    const monday = settings?.week_starts_monday !== false;
    const date = new Date(anchor);
    if (mode === 'month') date.setDate(1);
    const lead = (date.getDay() + (monday ? 6 : 0)) % 7;
    const count = mode === 'week' ? 7 : Math.ceil((lead + new Date(date.getFullYear(), date.getMonth()+1, 0).getDate()) / 7) * 7;
    const start = new Date(date); start.setDate(start.getDate() - lead);
    const days = Array.from({length:count}, (_, i) => new Date(start.getFullYear(), start.getMonth(), start.getDate()+i));
    const months = new Set(days.map(d => `${d.getFullYear()}-${d.getMonth()+1}`));
    const entries = await Promise.all([...months].map(pair => {
      const [year,month] = pair.split('-').map(Number);
      return call('calendar_month', {year,month}, 'Calendar');
    }));
    if (request !== version) return;
    const tasks = new Map(entries.flatMap(x => x ?? []).map(d => [d.date,d.tasks]));
    label.textContent = mode === 'month' ? date.toLocaleDateString(undefined,{month:'long',year:'numeric'})
      : `${days[0].toLocaleDateString(undefined,{month:'short',day:'numeric'})} – ${days[6].toLocaleDateString(undefined,{month:'short',day:'numeric'})}`;
    clear(weekdays); clear(grid);
    for (const n of monday ? [1,2,3,4,5,6,0] : [0,1,2,3,4,5,6]) weekdays.append(el('span',{text:['SUN','MON','TUE','WED','THU','FRI','SAT'][n]}));
    grid.classList.toggle('is-week', mode === 'week');
    for (const day of days) {
      const iso = toIso(day);
      const cell = el('div',{class:`widget-date${iso === toIso(now) ? ' is-today' : ''}${day.getMonth() !== date.getMonth() ? ' is-outside' : ''}`},[el('span',{class:'widget-date-number',text:String(day.getDate())})]);
      for (const task of tasks.get(iso) ?? []) {
        cell.append(el('span',{class:'widget-calendar-task',text:task.title,title:`${task.time ? task.time.slice(0,5)+' · ' : ''}${task.title}`}));
      }
      grid.append(cell);
    }
  }
  return load;
}
