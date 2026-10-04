const { test, before, after } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { chromium } = require(process.env.KAIROS_PLAYWRIGHT || 'playwright');
let browser;
before(async () => { browser = await chromium.launch({ headless: true, ...(process.env.KAIROS_BROWSER_CHANNEL ? { channel: process.env.KAIROS_BROWSER_CHANNEL } : {}) }); });
after(async () => { await browser?.close(); });

async function open(file = 'index.html', widget = false, view = 'habits') {
  const page = await browser.newPage();
  if (widget) await page.setViewportSize({width:650,height:300});
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.route('http://kairos.test/**', async route => {
    const name = new URL(route.request().url()).pathname.slice(1) || 'index.html';
    const content = await fs.readFile(path.join(__dirname, '../src', name));
    await route.fulfill({ body: content, contentType: name.endsWith('.js') ? 'text/javascript' : name.endsWith('.css') ? 'text/css' : 'text/html' });
  });
  await page.addInitScript((view) => {
    const listeners = {};
    window.calls = [];
    window.emit = event => (listeners[event] || []).forEach(fn => fn({ payload: null }));
    const task = { id: 'task1', title: 'Gym', notes: 'Draft', due: '2026-09-23', next: '2026-09-23', priority: 'None', tags: [], subtasks: [], completed_at: null, recurrence_label: 'every weekday' };
    const settings = { theme: 'light', parse_pills: true, hotkey_stays_open: true, calendar_page: true, habits_page: true, workout_page: true, show_completed: false };
    const days = Array.from({length: 30}, (_,i) => ({day:i+1,date:`2026-09-${String(i+1).padStart(2,'0')}`,is_future:i+1>23,is_today:i+1===23}));
    window.__TAURI__ = {
      event: { listen: async (event, fn) => { (listeners[event] ||= []).push(fn); return () => {}; } },
      window: { getCurrentWindow: () => ({ close: async () => {} }) },
      core: { invoke: async (cmd, args) => {
        window.calls.push({cmd,args});
        switch(cmd) {
          case 'today_date': return '2026-09-23';
          case 'current_month': case 'current_month_pair': return [2026,9];
          case 'settings_values': return settings;
          case 'habits': return [];
          case 'projects': return ['Work'];
          case 'tags': return [];
          case 'agenda': return [{id:'today',label:'Today',tasks:[task]}];
          case 'update_task': Object.assign(task,args.edit); window.emit('data-changed'); return task;
          case 'habit_month': return {label:'September 2026',days,habits:[{id:'habit1',name:'Gym',kind:'check',numeric:false,done:days.map(()=>false),count:0,streak:0}],archived:[]};
          case 'month_journal': return {entries:[{id:'journal1',title:'23 Sep',body:'My journal'}]};
          case 'save_journal_entry': window.emit('data-changed'); return {};
          case 'workout_page': await new Promise(r => setTimeout(r,150)); return {routine:'routine1',name:'Push',routines:[],exercises:[],sessions:[],max_sets:3};
          case 'calendar_month': return [{date:'2026-09-23',tasks:[task]}];
          case 'widget_context': return {view,project:null,routine:null,pinned:false};
          case 'widget_layout': return {restore_on_start:true,widgets:[]};
          case 'save_widget_layout': return {restore_on_start:args.restoreOnStart,widgets:[{}]};
          case 'pin_widget': return true;
          case 'preview_line': {
            const start = args.line.indexOf('tomorrow');
            const a = new TextEncoder().encode(args.line.slice(0,start)).length;
            return {title:args.line,due:'2026-09-24',spans:start < 0 || args.excluded?.length ? [] : [{start:a,end:a+8,field:'date'}]};
          }
          case 'quick_add': return {title:args.line};
          case 'settings': return [];
          case 'vault_info': return {enabled:true,files:1,path:'test',settings_file:'settings.json'};
          default: return null;
        }
      } }
    };
  }, view);
  const url = new URL(file, 'http://kairos.test/');
  if (widget && !url.hash) url.searchParams.set('widget', '1');
  await page.goto(url.href);
  await page.waitForTimeout(120);
  return { page, errors };
}

test('search keeps focus after results refresh', async () => {
  const {page,errors} = await open();
  await page.locator('.search').fill('gym');
  await page.waitForTimeout(250);
  assert.equal(await page.locator('.search').evaluate(el => el === document.activeElement),true);
  assert.deepEqual(errors,[]); await page.close();
});

test('late workout load cannot replace Habits toolbar', async () => {
  const {page,errors} = await open();
  await page.locator('#page-workout').click();
  await page.waitForTimeout(30);
  await page.locator('#page-habits').click();
  await page.waitForTimeout(350);
  assert.equal(await page.locator('#status-tools').textContent(),'');
  assert.equal(await page.locator('#here').textContent(),'September 2026');
  assert.deepEqual(errors,[]); await page.close();
});

test('task notes survive background refresh while focused', async () => {
  const {page,errors} = await open();
  await page.locator('li.task .title').click();
  await page.locator('.note-field').fill('Unsaved writing');
  await page.evaluate(() => window.emit('data-changed'));
  await page.waitForTimeout(150);
  assert.equal(await page.locator('.note-field').inputValue(),'Unsaved writing');
  assert.equal(await page.locator('.note-field').evaluate(el => el === document.activeElement),true);
  assert.deepEqual(errors,[]); await page.close();
});

test('journal autosave preserves the active textarea', async () => {
  const {page,errors} = await open();
  await page.locator('#page-habits').click();
  await page.locator('.entry textarea').fill('Writing continuously');
  await page.waitForTimeout(850);
  assert.equal(await page.locator('.entry textarea').inputValue(),'Writing continuously');
  assert.equal(await page.locator('.entry textarea').evaluate(el => el === document.activeElement),true);
  assert.deepEqual(errors,[]); await page.close();
});

test('emoji spans and leading-space exclusions match submitted input', async () => {
  const {page,errors} = await open();
  const input = page.locator('#quick-add');
  await input.fill('  🏋 gym tomorrow');
  await page.waitForTimeout(200);
  assert.equal(await page.locator('.tok-when').textContent(),'tomorrow');
  await input.press('End'); await input.press('Backspace'); await page.waitForTimeout(120);
  await input.press('Enter');
  const call = await page.evaluate(() => window.calls.findLast(c => c.cmd === 'quick_add'));
  assert.equal(call.args.line,'  🏋 gym tomorrow');
  assert.deepEqual(call.args.excluded, [[11,19]]);
  assert.deepEqual(errors,[]); await page.close();
});

test('quick add respects keep-open and theme settings', async () => {
  const {page,errors} = await open('quick-add.html');
  await page.locator('#line').fill('task'); await page.locator('#line').press('Enter');
  await page.waitForTimeout(100);
  assert.equal(await page.evaluate(() => window.calls.some(c => c.cmd === 'hide_window')),false);
  assert.equal(await page.locator('html').getAttribute('data-theme'),'light');
  assert.deepEqual(errors,[]); await page.close();
});

test('habit widget is compact and supports tracking and pinning without manual save', async () => {
  const {page,errors} = await open('widget.html',true);
  await page.waitForTimeout(150);
  assert.equal(await page.locator('.widget-habits').isVisible(),true);
  assert.equal(await page.locator('.rail').isVisible(),false);
  await page.locator('#pin').click();
  assert.equal(await page.locator('#pin').getAttribute('aria-pressed'),'true');
  assert.equal(await page.locator('.widget-habit-stats').isVisible(),false);
  await page.setViewportSize({width:1050,height:400});
  assert.equal(await page.locator('.widget-habit-stats').isVisible(),true);
  assert.equal(await page.locator('#capture').isVisible(), false);
  assert.equal(await page.locator('#journal').isVisible(), false);
  assert.equal(await page.getByRole('button',{name:'Save layout',exact:true}).count(), 0);
  await page.getByRole('button',{name:'Gym on 2026-09-23',exact:true}).click();
  await page.waitForTimeout(100);
  assert.equal(await page.evaluate(() => window.calls.some(c => c.cmd === 'toggle_habit' && c.args.id === 'habit1' && c.args.date === '2026-09-23')),true);
  assert.deepEqual(errors,[]); await page.close();
});

test('calendar widget switches between month and weeks across month boundaries', async () => {
  const {page,errors} = await open('widget.html', true, 'calendar');
  assert.equal(await page.locator('#capture').isVisible(), false);
  assert.ok(await page.locator('.widget-date').count() >= 28);
  await page.locator('.widget-calendar-task').first().click();
  assert.equal(await page.locator('.widget-calendar').isVisible(),true);
  assert.equal(await page.evaluate(() => window.calls.some(c => c.cmd === 'update_widget_context')),false);
  await page.getByRole('button',{name:'week',exact:true}).click();
  await page.waitForTimeout(100);
  assert.equal(await page.locator('.widget-date').count(), 7);
  await page.getByRole('button',{name:'Next',exact:true}).click();
  await page.waitForTimeout(100);
  assert.equal(await page.locator('.widget-date-number').count(), 7);
  assert.equal(await page.evaluate(() => window.calls.some(c => c.cmd === 'calendar_month' && c.args.month === 10)),true);
  await page.getByRole('button',{name:'month',exact:true}).click();
  await page.waitForTimeout(100);
  assert.ok(await page.locator('.widget-date').count() >= 28);
  assert.deepEqual(errors,[]); await page.close();
});

test('the rail and W open one widget picker without a duplicate action', async () => {
  const { page, errors } = await open();
  assert.equal(await page.getByText('Open as widget',{exact:true}).count(),0);
  await page.locator('#sticky-toggle').click();
  await page.getByRole('dialog',{name:'Widgets'}).getByRole('button',{name:'Tasks',exact:true}).click();
  assert.equal(await page.evaluate(() => window.calls.some(c => c.cmd === 'toggle_widget_command')),true);
  await page.locator('#quick-add').blur();
  await page.keyboard.press('w');
  await page.getByRole('dialog',{name:'Widgets'}).getByRole('button',{name:'Calendar',exact:true}).click();
  assert.equal(await page.evaluate(() => window.calls.some(c => c.cmd === 'create_widget' && c.args.view === 'calendar')),true);
  assert.deepEqual(errors, []);
  await page.close();
});
