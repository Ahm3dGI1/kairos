import { call, clear, el, invoke, listen, parseDate, today } from './shared.js';

const list = document.getElementById('list');
const counts = document.getElementById('counts');
const pin = document.getElementById('pin');
let todayDate = new Date();

async function refresh() {
  todayDate = (await today()) ?? new Date();
  const tasks = (await call('list_tasks', { filter: 'Today', sort: 'Due' }, 'Sticky note')) ?? [];

  const due = tasks.filter((t) => !t.completed_at).length;
  const over = tasks.filter((t) => t.overdue).length;
  counts.textContent = due ? `${due} due${over ? ` · ${over} over` : ''}` : 'clear';

  clear(list);
  if (!tasks.length) {
    list.appendChild(el('li', { class: 'sticky-empty', text: 'nothing due today' }));
    return;
  }

  for (const task of tasks) {
    const done = Boolean(task.completed_at);
    const classes = [];
    if (task.overdue) classes.push('late');
    else classes.push('now');
    if (done) classes.push('done');

    // Overdue counts in days; everything else shows its time.
    let when = '';
    if (task.overdue) {
      const days = Math.round((parseDate(task.next ?? task.due) - todayDate) / 86400000);
      when = `${days}d`;
    } else if (task.time) {
      when = task.time.slice(0, 5);
    }

    list.appendChild(
      el('li', { class: classes.join(' ') }, [
        el('button', {
          class: `check ${done ? 'on' : ''}`.trim(),
          type: 'button',
          'aria-label': `Complete: ${task.title}`,
          onclick: async () => {
            await call('complete_task', { id: task.id }, 'Complete');
            refresh();
          },
        }),
        el('span', { class: 'title', text: task.title }),
        el('span', { class: 'when', text: when }),
      ]),
    );
  }
}

function showPin(pinned) {
  pin.classList.toggle('on', pinned);
  pin.title = pinned ? 'Unpin — let other windows cover it' : 'Pin above other windows';
  pin.setAttribute('aria-pressed', String(pinned));
}

pin.addEventListener('click', async () => {
  showPin(Boolean(await call('toggle_sticky_pin', undefined, 'Pin')));
});
document.getElementById('open').addEventListener('click', () => invoke('show_main_window'));
document.getElementById('close').addEventListener('click', () =>
  invoke('hide_window', { label: 'widget' }),
);

call('sticky_pinned', undefined, 'Pin').then((pinned) => showPin(Boolean(pinned)));
listen('data-changed', refresh);
refresh();

// Catch the date rolling over while the note sits open for days.
setInterval(refresh, 5 * 60 * 1000);
