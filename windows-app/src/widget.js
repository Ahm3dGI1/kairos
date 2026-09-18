// The sticky note.
//
// Small, read-mostly — the one thing you can do here is tick something off —
// and it never steals focus. By default it behaves like any other window and
// drops behind whatever you focus next; the pin makes it stay on top, which is
// a deliberate choice rather than the default because always-on-top is
// intrusive by nature.

import { call, clear, el, formatDue, formatTime, invoke, listen, shortTime, today } from './shared.js';

const list = document.getElementById('list');
const summary = document.getElementById('summary');
let todayDate = new Date();

async function refresh() {
  todayDate = (await today()) ?? new Date();
  const tasks = (await call('list_tasks', { filter: 'Today', sort: 'Due' }, 'Widget')) ?? [];
  summary.textContent = (await call('summary', undefined, 'Widget')) ?? '';

  clear(list);
  if (!tasks.length) {
    list.appendChild(el('li', { class: 'widget-empty', text: 'Nothing due today.' }));
    return;
  }

  for (const task of tasks) {
    const check = el('button', {
      class: 'check',
      type: 'button',
      text: '✓',
      title: 'Complete',
      onclick: async () => {
        await call('complete_task', { id: task.id }, 'Complete');
        refresh();
      },
    });

    const when = [];
    if (task.overdue) when.push('Overdue');
    else if (task.time) when.push(formatTime(shortTime(task.time)));
    else if (task.next) when.push(formatDue(task.next, todayDate));

    list.appendChild(
      el('li', {}, [
        check,
        el('span', { text: task.title }),
        when.length
          ? el('span', { class: `when ${task.overdue ? 'overdue' : ''}`.trim(), text: when[0] })
          : null,
      ]),
    );
  }
}

document.getElementById('open').addEventListener('click', () => invoke('show_main_window'));
document.getElementById('close').addEventListener('click', () =>
  invoke('hide_window', { label: 'widget' }),
);

const pin = document.getElementById('pin');

function showPin(pinned) {
  pin.classList.toggle('on', pinned);
  pin.title = pinned ? 'Unpin — let other windows cover it' : 'Pin above other windows';
  pin.setAttribute('aria-pressed', String(pinned));
}

pin.addEventListener('click', async () => {
  showPin(Boolean(await call('toggle_sticky_pin', undefined, 'Pin')));
});

call('sticky_pinned', undefined, 'Pin').then((pinned) => showPin(Boolean(pinned)));

listen('data-changed', refresh);
refresh();

// Catch the date rolling over while the widget sits open for days.
setInterval(refresh, 5 * 60 * 1000);
