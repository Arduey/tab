/* 首页交互：搜索过滤、键盘导航、主题切换。
 * 无框架、无构建；用事件委托把开销压到最低。 */
(function () {
  'use strict';

  var prefs = window.__TAB__ || {};

  // ---------------- 主题 ----------------
  var THEMES = ['auto', 'light', 'dark'];
  var root = document.documentElement;

  function applyTheme(t) {
    if (t === 'auto') root.removeAttribute('data-theme');
    else root.setAttribute('data-theme', t);
    var meta = document.querySelector('meta[name="theme-color"]');
    if (meta) meta.remove();
  }

  var stored = null;
  try { stored = localStorage.getItem('tab.theme'); } catch (e) { /* 隐私模式 */ }
  applyTheme(stored || prefs.theme || 'auto');

  var themeBtn = document.getElementById('theme-btn');
  if (themeBtn) {
    themeBtn.addEventListener('click', function () {
      var cur = stored || prefs.theme || 'auto';
      var next = THEMES[(THEMES.indexOf(cur) + 1) % THEMES.length];
      stored = next;
      applyTheme(next);
      try { localStorage.setItem('tab.theme', next); } catch (e) {}
    });
  }

  // 主色
  if (prefs.accent && /^#[0-9a-f]{3,8}$/i.test(prefs.accent)) {
    root.style.setProperty('--accent', prefs.accent);
  }

  // ---------------- 字母兜底图标 ----------------
  window.__tabFallback = function (name) {
    var ch = (name || '#').trim().charAt(0).toUpperCase() || '#';
    var el = document.createElement('span');
    el.className = 'letter-icon';
    el.style.width = '26px';
    el.style.height = '26px';
    el.style.fontSize = '13px';
    el.textContent = ch;
    return el;
  };

  // ---------------- 搜索 ----------------
  var input = document.getElementById('search');
  var results = document.getElementById('search-results');
  if (!input || !results) return;

  // 一次扫描建索引，之后只在内存里过滤
  var cards = Array.prototype.slice.call(document.querySelectorAll('.card'));
  var index = cards.map(function (el) {
    return {
      el: el,
      name: (el.dataset.name || '').toLowerCase(),
      url: (el.dataset.url || '').toLowerCase(),
      desc: (el.dataset.desc || '').toLowerCase(),
      cat: (el.closest('.cat') && el.closest('.cat').dataset.catName || '').toLowerCase()
    };
  });

  var active = -1;
  var matches = [];
  var timer = 0;

  function clearResults() {
    matches = [];
    active = -1;
    results.hidden = true;
    results.textContent = '';
  }

  // 生成一行搜索结果
  function resultRow(opts) {
    var row = document.createElement('div');
    row.className = 'sr';
    row.setAttribute('role', 'option');
    if (opts.search) row.dataset.search = opts.search;
    else row.dataset.href = opts.href;

    if (opts.icon) {
      var icon = opts.icon.cloneNode(true);
      icon.className = 'card-icon';
      row.appendChild(icon);
    }
    var body = document.createElement('span');
    body.style.minWidth = '0';
    var name = document.createElement('span');
    name.className = 'sr-name';
    name.textContent = opts.name;
    body.appendChild(name);
    if (opts.meta) {
      var meta = document.createElement('small');
      meta.textContent = opts.meta;
      body.appendChild(meta);
    }
    row.appendChild(body);
    return row;
  }

  function render(list, query) {
    var frag = document.createDocumentFragment();

    list.slice(0, 40).forEach(function (item) {
      frag.appendChild(resultRow({
        href: item.el.getAttribute('href'),
        icon: item.el.querySelector('.card-icon'),
        name: item.el.dataset.name || '',
        meta: item.catName ? ' · ' + item.catName : ''
      }));
    });

    // 末尾固定一条「搜索 xxx」，回车或点击都用配置的搜索引擎
    if (query) {
      frag.appendChild(resultRow({ search: query, name: '搜索「' + query + '」' }));
    }

    results.textContent = '';
    var count = frag.childNodes.length;
    results.appendChild(frag);
    results.hidden = count === 0;
  }

  function search(q) {
    q = q.trim().toLowerCase();
    if (!q) {
      clearResults();
      return;
    }
    var out = [];
    for (var i = 0; i < index.length; i++) {
      var it = index[i];
      if (it.name.indexOf(q) !== -1 || it.url.indexOf(q) !== -1 || it.desc.indexOf(q) !== -1 || it.cat.indexOf(q) !== -1) {
        out.push(it);
        if (out.length >= 200) break;
      }
    }
    matches = out.map(function (it) {
      var cat = it.el.closest('.cat');
      return { el: it.el, catName: cat ? cat.dataset.catName : '' };
    });
    active = out.length ? 0 : -1;
    render(matches.slice(), q);
    highlight();
  }

  function highlight() {
    var rows = results.querySelectorAll('.sr');
    for (var i = 0; i < rows.length; i++) {
      rows[i].setAttribute('aria-selected', i === active ? 'true' : 'false');
    }
    var cur = rows[active];
    if (cur && cur.scrollIntoView) cur.scrollIntoView({ block: 'nearest' });
  }

  input.addEventListener('input', function () {
    // 用 setTimeout(0) 而不是 requestAnimationFrame：后台标签页里 rAF 会被
    // 完全暂停，搜索就永远不刷新；setTimeout 只是节流到 1s，仍然可用。
    if (timer) clearTimeout(timer);
    timer = setTimeout(function () {
      timer = 0;
      search(input.value);
    }, 0);
  });

  input.addEventListener('keydown', function (e) {
    if (e.key === 'Escape') {
      input.value = '';
      clearResults();
      input.blur();
      return;
    }
    if (!matches.length) {
      // 没有任何标签匹配时，回车 = 用搜索引擎查这个词
      if (e.key === 'Enter' && input.value.trim()) {
        e.preventDefault();
        goSearch(input.value.trim());
      }
      return;
    }
    // 末位固定是「搜索 xxx」条目
    var searchIndex = matches.length;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      active = (active + 1) % (searchIndex + 1);
      highlight();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      active = (active - 1 + searchIndex + 1) % (searchIndex + 1);
      highlight();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (active === searchIndex) {
        goSearch(input.value.trim());
      } else {
        var pick = matches[active];
        if (pick) window.open(pick.el.getAttribute('href'), prefs.target === '1' ? '_blank' : '_self');
      }
    }
  });

  function goSearch(q) {
    var engine = prefs.engine || 'https://www.google.com/search?q=%s';
    var url = engine.indexOf('%s') >= 0
      ? engine.replace('%s', encodeURIComponent(q))
      : engine + encodeURIComponent(q);
    window.open(url, prefs.target === '1' ? '_blank' : '_self');
  }

  results.addEventListener('click', function (e) {
    var row = e.target.closest('.sr');
    if (!row) return;
    if (row.dataset.search != null) {
      goSearch(row.dataset.search);
      return;
    }
    window.location.href = row.dataset.href;
  });

  document.addEventListener('click', function (e) {
    if (!results.hidden && !results.contains(e.target) && e.target !== input) clearResults();
  });

  // 快捷键：/ 或 Ctrl/Cmd+K 聚焦搜索
  document.addEventListener('keydown', function (e) {
    var typing = /^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement.tagName);
    if ((e.key === '/' && !typing) || ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k')) {
      e.preventDefault();
      input.focus();
      input.select();
    }
  });

  // 点击卡片时记录一次热度（失败无所谓，不阻塞跳转）
  document.addEventListener('pointerdown', function (e) {
    var card = e.target.closest && e.target.closest('.card');
    if (!card || !card.dataset.id) return;
    var url = '/api/tags/' + card.dataset.id + '/hit';
    if (navigator.sendBeacon) navigator.sendBeacon(url, new Blob([]));
  }, { passive: true });
})();