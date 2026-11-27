/* 管理台：分类/标签 CRUD、拖拽排序、导入导出、token、站点设置。
 * 全部 DOM 操作走事件委托 + 局部重渲染，避免整页刷新。 */
(function () {
  'use strict';

  var boot = {};
  var raw = document.getElementById('bootstrap');
  if (raw) {
    try { boot = JSON.parse(raw.textContent || '{}'); } catch (e) { boot = {}; }
  }
  var state = {
    categories: boot.categories || [],
    tags: boot.tags || [],
    settings: boot.settings || {},
    tokens: boot.tokens || [],
    icons: boot.builtin_icons || {},
  };

  // ---------------- 工具 ----------------
  function $(sel, root) { return (root || document).querySelector(sel); }
  function $$(sel, root) { return Array.prototype.slice.call((root || document).querySelectorAll(sel)); }

  function el(tag, attrs, children) {
    var node = document.createElement(tag);
    if (attrs) {
      Object.keys(attrs).forEach(function (k) {
        if (k === 'class') node.className = attrs[k];
        else if (k === 'text') node.textContent = attrs[k];
        else if (k === 'html') node.innerHTML = attrs[k];
        else if (k.indexOf('on') === 0) node.addEventListener(k.slice(2), attrs[k]);
        else if (attrs[k] != null) node.setAttribute(k, attrs[k]);
      });
    }
    (children || []).forEach(function (c) {
      if (c) node.appendChild(typeof c === 'string' ? document.createTextNode(c) : c);
    });
    return node;
  }

  var toastTimer = 0;
  function toast(msg) {
    var t = document.getElementById('toast');
    if (!t) return;
    t.textContent = msg;
    t.hidden = false;
    requestAnimationFrame(function () { t.classList.add('show'); });
    clearTimeout(toastTimer);
    toastTimer = setTimeout(function () {
      t.classList.remove('show');
      setTimeout(function () { t.hidden = true; }, 200);
    }, 1800);
  }

  function api(method, path, body, isForm) {
    var opts = { method: method, headers: {}, credentials: 'same-origin' };
    if (body != null) {
      if (isForm) opts.body = body;
      else {
        opts.headers['Content-Type'] = 'application/json';
        opts.body = JSON.stringify(body);
      }
    }
    return fetch(path, opts).then(function (r) {
      return r.json().catch(function () { return { ok: false, error: 'HTTP ' + r.status }; });
    });
  }

  function svgIcon(name, size) {
    // 内置图标由后端渲染，这里只用字母兜底
    return null;
  }

  function letterIcon(name, size) {
    var ch = (name || '#').trim().charAt(0).toUpperCase() || '#';
    return el('span', { class: 'letter-icon', text: ch, style: 'width:' + size + 'px;height:' + size + 'px;font-size:' + Math.round(size * 0.52) + 'px' });
  }

  function tagIcon(tag, size) {
    var icon = (tag.icon || '').trim();
    if (!icon || icon === 'favicon') {
      var img = el('img', {
        src: '/api/favicon/' + tag.id,
        width: size, height: size, loading: 'lazy', alt: ''
      });
      img.addEventListener('error', function () {
        img.replaceWith(letterIcon(tag.name, size));
      });
      return img;
    }
    if (icon.indexOf('upload:') === 0) {
      return el('img', { src: '/icons/' + icon.slice(7), width: size, height: size, loading: 'lazy', alt: '' });
    }
    if (icon.indexOf('builtin:') === 0) {
      var key = icon.slice(8);
      var svg = state.icons[key];
      if (svg) {
        var span = el('span', { class: 'card-icon', html: svg });
        return span.firstChild || letterIcon(tag.name, size);
      }
      return letterIcon(tag.name, size);
    }
    return el('img', { src: icon, width: size, height: size, loading: 'lazy', alt: '' });
  }

  // ---------------- 渲染分类与标签 ----------------
  function renderCats() {
    var host = document.getElementById('cats');
    if (!host) return;
    var frag = document.createDocumentFragment();
    state.categories.forEach(function (cat) {
      var tags = state.tags.filter(function (t) { return t.category_id === cat.id; });
      var head = el('div', { class: 'mcat-head' }, [
        el('h3', {}, [
          el('span', { class: 'cat-dot', style: 'background:' + (cat.color || 'var(--accent)') }),
          document.createTextNode(cat.name)
        ]),
        cat.builtin ? el('span', { class: 'badge', text: '内置' }) : null,
        el('span', { class: 'spacer' }),
        el('button', { class: 'btn small', type: 'button', 'data-act': 'add-tag', 'data-cat': String(cat.id), text: '+ 标签' }),
        cat.builtin ? null : el('button', { class: 'mini', type: 'button', 'data-act': 'edit-cat', 'data-cat': String(cat.id), title: '重命名', text: '✎' }),
        cat.builtin ? null : el('button', { class: 'mini', type: 'button', 'data-act': 'del-cat', 'data-cat': String(cat.id), title: '删除分类', text: '🗑' })
      ]);
      var body = el('div', { class: 'mcat-body', 'data-drop': String(cat.id) });
      tags.forEach(function (tag) {
        var row = el('div', { class: 'mtag', draggable: 'true', 'data-tag': String(tag.id) }, [
          el('span', { class: 'card-icon' }, [tagIcon(tag, 18)]),
          el('span', { class: 'mtag-name', text: tag.name, title: tag.url }),
          el('span', { class: 'mtag-actions' }, [
            el('button', { class: 'mini', type: 'button', 'data-act': 'refetch', 'data-tag': String(tag.id), title: '重新抓取图标', text: '⟳' }),
            el('button', { class: 'mini', type: 'button', 'data-act': 'edit-tag', 'data-tag': String(tag.id), title: '编辑', text: '✎' }),
            el('button', { class: 'mini', type: 'button', 'data-act': 'del-tag', 'data-tag': String(tag.id), title: '删除', text: '🗑' })
          ])
        ]);
        body.appendChild(row);
      });
      var section = el('section', { class: 'mcat' + (tags.length ? '' : ' empty') }, [head, body]);
      frag.appendChild(section);
    });
    host.textContent = '';
    host.appendChild(frag);
  }

  // ---------------- 拖拽 ----------------
  var dragId = 0;
  document.addEventListener('dragstart', function (e) {
    var tag = e.target.closest && e.target.closest('.mtag');
    if (!tag) return;
    dragId = Number(tag.dataset.tag);
    tag.classList.add('dragging');
    e.dataTransfer.effectAllowed = 'move';
    // Firefox 需要 setData 才会开始拖拽
    try { e.dataTransfer.setData('text/plain', String(dragId)); } catch (err) {}
  });

  document.addEventListener('dragend', function (e) {
    var tag = e.target.closest && e.target.closest('.mtag');
    if (tag) tag.classList.remove('dragging');
    $$('.drop-target').forEach(function (n) { n.classList.remove('drop-target'); });
    dragId = 0;
  });

  document.addEventListener('dragover', function (e) {
    var zone = e.target.closest && e.target.closest('[data-drop]');
    if (!zone || !dragId) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    $$('.drop-target').forEach(function (n) { if (n !== zone) n.classList.remove('drop-target'); });
    zone.classList.add('drop-target');
  });

  document.addEventListener('drop', function (e) {
    var zone = e.target.closest && e.target.closest('[data-drop]');
    if (!zone || !dragId) return;
    e.preventDefault();
    var catId = Number(zone.dataset.drop);
    var moved = state.tags.find(function (t) { return t.id === dragId; });
    if (!moved) return;
    moved.category_id = catId;

    // 决定插入位置：拖到某个卡片上就插在它前面
    var overRow = e.target.closest('.mtag');
    var siblings = state.tags.filter(function (t) { return t.category_id === catId && t.id !== moved.id; });
    var idx = siblings.length;
    if (overRow) {
      var overId = Number(overRow.dataset.tag);
      var at = siblings.findIndex(function (t) { return t.id === overId; });
      if (at >= 0) idx = at;
    }
    siblings.splice(idx, 0, moved);

    // 重排后统一编号，一次请求提交
    var payload = [];
    siblings.forEach(function (t, i) {
      t.sort = (i + 1) * 10;
      payload.push({ id: t.id, category_id: catId, sort: t.sort });
    });
    zone.classList.remove('drop-target');
    renderCats();
    api('POST', '/api/tags/reorder', { tags: payload }).then(function (r) {
      if (!r.ok) toast('排序保存失败');
    });
    dragId = 0;
  });

  // ---------------- 模态框 ----------------
  var modalRoot = document.getElementById('modal-root');

  function closeModal() {
    if (!modalRoot) return;
    modalRoot.hidden = true;
    modalRoot.textContent = '';
  }

  function openModal(title, nodes, onOk, okText) {
    if (!modalRoot) return;
    var form = el('form', { class: 'stack' }, nodes);
    var okBtn;

    function submit() {
      if (okBtn.disabled) return;
      if (!form.checkValidity()) {
        // 让浏览器把第一个不合法的字段高亮出来
        form.reportValidity();
        return;
      }
      okBtn.disabled = true;
      onOk(new FormData(form), form);
      // 失败时（比如服务端拒绝）允许再点一次
      setTimeout(function () { okBtn.disabled = false; }, 3000);
    }

    // 用 type=button + 显式绑定，避免不同浏览器下隐式提交行为不一致
    okBtn = el('button', { class: 'btn primary', type: 'button', text: okText || '保存', onclick: submit });
    var foot = el('div', { class: 'modal-foot' }, [
      el('button', { class: 'btn', type: 'button', text: '取消', onclick: closeModal }),
      okBtn
    ]);
    form.appendChild(foot);
    var box = el('div', { class: 'modal' }, [el('h3', { text: title }), form]);
    modalRoot.textContent = '';
    modalRoot.appendChild(box);
    modalRoot.hidden = false;
    form.addEventListener('submit', function (e) {
      e.preventDefault();
      submit();
    });
    var first = form.querySelector('input,select');
    if (first) first.focus();
  }

  modalRoot && modalRoot.addEventListener('click', function (e) {
    if (e.target === modalRoot) closeModal();
  });

  document.addEventListener('keydown', function (e) {
    if (e.key === 'Escape') closeModal();
  });

  // ---------------- 分类 ----------------
  function catOptions(selected) {
    var sel = el('select', { name: 'category_id' });
    state.categories.forEach(function (c) {
      var o = el('option', { value: String(c.id), text: c.name });
      if (c.id === selected) o.selected = true;
      sel.appendChild(o);
    });
    return sel;
  }

  function askCategory(cat) {
    var nameInput = el('input', { name: 'name', value: cat ? cat.name : '', maxlength: '40', required: 'required', placeholder: '分类名称' });
    var colorInput = el('input', { name: 'color', type: 'color', value: (cat && cat.color) || '#3b82f6' });
    openModal(cat ? '编辑分类' : '新建分类', [
      el('label', {}, [document.createTextNode('名称'), nameInput]),
      el('label', {}, [document.createTextNode('颜色'), colorInput])
    ], function (fd) {
      var payload = { name: String(fd.get('name') || '').trim(), color: String(fd.get('color') || '') };
      if (!payload.name) return toast('名称不能为空');
      var req = cat
        ? api('PUT', '/api/categories/' + cat.id, payload)
        : api('POST', '/api/categories', payload);
      req.then(function (r) {
        if (!r.ok) return toast(r.error || '操作失败');
        closeModal();
        toast(cat ? '已更新' : '已创建');
        location.reload();
      });
    });
  }

  // ---------------- 标签 ----------------
  var ICON_KEYS = Object.keys(state.icons);

  function pickIconRow(current, onPick) {
    var wrap = el('div', { class: 'icon-picker' });
    var values = ['favicon'].concat(ICON_KEYS.map(function (k) { return 'builtin:' + k; }));
    values.forEach(function (v) {
      var btn = el('button', { type: 'button', title: v, class: v === current ? 'sel' : '' });
      if (v === 'favicon') {
        btn.appendChild(el('span', { class: 'letter-icon', text: 'A', style: 'width:20px;height:20px;font-size:11px' }));
      } else {
        var name = v.slice(8);
        btn.innerHTML = state.icons[name] || name.slice(0, 2);
      }
      btn.addEventListener('click', function () {
        $$('button', wrap).forEach(function (b) { b.classList.remove('sel'); });
        btn.classList.add('sel');
        onPick(v);
      });
      wrap.appendChild(btn);
    });
    return wrap;
  }

  function askTag(tag, presetCatId) {
    var iconValue = { v: tag ? (tag.icon || 'favicon') : 'favicon' };

    var name = el('input', { name: 'name', value: tag ? tag.name : '', maxlength: '80', required: 'required', placeholder: '显示名称' });
    var url = el('input', { name: 'url', value: tag ? tag.url : '', required: 'required', placeholder: 'https://example.com' });
    var desc = el('input', { name: 'description', value: tag ? tag.description : '', maxlength: '120', placeholder: '描述（可选）' });
    var cat = catOptions(tag ? tag.category_id : (presetCatId || (state.categories[0] && state.categories[0].id)));
    cat.setAttribute('name', 'category_id');

    var preview = el('span', { class: 'card-icon' });
    function refreshPreview() {
      preview.textContent = '';
      if (iconValue.v === 'favicon') {
        preview.appendChild(letterIcon(name.value || 'A', 22));
      } else if (iconValue.v.indexOf('builtin:') === 0) {
        preview.innerHTML = state.icons[iconValue.v.slice(8)] || '';
      } else if (iconValue.v.indexOf('upload:') === 0) {
        preview.appendChild(el('img', { src: '/icons/' + iconValue.v.slice(7), width: 22, height: 22, alt: '' }));
      }
    }
    name.addEventListener('input', refreshPreview);

    var uploadInput = el('input', { type: 'file', accept: 'image/*' });
    uploadInput.addEventListener('change', function () {
      var f = uploadInput.files && uploadInput.files[0];
      if (!f) return;
      var fd = new FormData();
      fd.append('file', f);
      fetch('/api/icon/upload', { method: 'POST', body: fd, credentials: 'same-origin' })
        .then(function (r) { return r.json(); })
        .then(function (j) {
          if (!j.ok) return toast(j.error || '上传失败');
          iconValue.v = j.data.icon;
          refreshPreview();
          toast('图标已上传');
        });
    });

    var picker = pickIconRow(iconValue.v, function (v) {
      iconValue.v = v;
      refreshPreview();
    });
    refreshPreview();

    openModal(tag ? '编辑标签' : '新建标签', [
      el('label', {}, [document.createTextNode('名称'), name]),
      el('label', {}, [document.createTextNode('链接'), url]),
      el('label', {}, [document.createTextNode('分类'), cat]),
      el('label', {}, [document.createTextNode('描述'), desc]),
      el('label', {}, [
        el('span', { style: 'display:flex;align-items:center;gap:8px' }, [
          document.createTextNode('图标'), preview
        ]),
        picker,
        el('span', { class: 'muted small', text: '默认自动抓取网站 favicon；也可选内置图标或上传图片。' }),
        uploadInput
      ])
    ], function (fd) {
      var payload = {
        name: String(fd.get('name') || '').trim(),
        url: String(fd.get('url') || '').trim(),
        description: String(fd.get('description') || '').trim(),
        category_id: Number(fd.get('category_id')),
        icon: iconValue.v === 'favicon' ? '' : iconValue.v
      };
      if (!payload.name || !payload.url) return toast('名称和链接都要填');
      var req = tag
        ? api('PUT', '/api/tags/' + tag.id, payload)
        : api('POST', '/api/tags', payload);
      req.then(function (r) {
        if (!r.ok) return toast(r.error || '保存失败');
        closeModal();
        toast(tag ? '已更新' : '已添加');
        setTimeout(function () { location.reload(); }, 220);
      });
    });
  }

  // ---------------- 事件委托 ----------------
  document.addEventListener('click', function (e) {
    var btn = e.target.closest('[data-act]');
    if (!btn) return;
    var act = btn.dataset.act;
    var catId = Number(btn.dataset.cat || 0);
    var tagId = Number(btn.dataset.tag || 0);

    if (act === 'add-cat') return askCategory(null);
    if (act === 'edit-cat') {
      var c = state.categories.find(function (x) { return x.id === catId; });
      return askCategory(c);
    }
    if (act === 'del-cat') {
      if (!confirm('删除这个分类？分类下的标签会一起删除。')) return;
      return api('DELETE', '/api/categories/' + catId).then(function (r) {
        if (!r.ok) return toast(r.error || '删除失败');
        location.reload();
      });
    }
    if (act === 'add-tag') {
      var preset = catId && state.categories.some(function (x) { return x.id === catId; }) ? catId : 0;
      return askTag(null, preset);
    }
    if (act === 'edit-tag') {
      var tg = state.tags.find(function (x) { return x.id === tagId; });
      return askTag(tg);
    }
    if (act === 'del-tag') {
      if (!confirm('删除这个标签？')) return;
      return api('DELETE', '/api/tags/' + tagId).then(function (r) {
        if (!r.ok) return toast(r.error || '删除失败');
        location.reload();
      });
    }
    if (act === 'refetch') {
      btn.disabled = true;
      return api('POST', '/api/tags/' + tagId + '/refetch-icon', {}).then(function (r) {
        btn.disabled = false;
        toast(r.ok && r.data.fetched ? '图标已更新' : '没抓到图标，可能站点限制了');
      });
    }
  });

  document.getElementById('add-cat') && document.getElementById('add-cat').addEventListener('click', function () { askCategory(null); });
  document.getElementById('add-tag') && document.getElementById('add-tag').addEventListener('click', function () { askTag(null); });

  // ---------------- 标签页切换 ----------------
  $$('.tab').forEach(function (tab) {
    tab.addEventListener('click', function () {
      $$('.tab').forEach(function (t) { t.classList.remove('active'); });
      $$('.panel').forEach(function (p) { p.classList.remove('active'); });
      tab.classList.add('active');
      var panel = document.getElementById('panel-' + tab.dataset.tab);
      if (panel) panel.classList.add('active');
    });
  });

  // ---------------- 退出 ----------------
  var logoutBtn = document.getElementById('logout-btn');
  logoutBtn && logoutBtn.addEventListener('click', function () {
    fetch('/logout', { method: 'POST', credentials: 'same-origin' }).then(function () {
      location.href = '/';
    });
  });

  // ---------------- 导出 / 导入 ----------------
  var exportBtn = document.getElementById('export-btn');
  exportBtn && exportBtn.addEventListener('click', function () {
    location.href = '/api/export';
  });

  var importBtn = document.getElementById('import-btn');
  importBtn && importBtn.addEventListener('click', function () {
    var file = document.getElementById('import-file').files[0];
    var msg = document.getElementById('import-msg');
    if (!file) return toast('先选一个 JSON 文件');
    var reader = new FileReader();
    reader.onload = function () {
      var data;
      try { data = JSON.parse(String(reader.result)); } catch (e) { return toast('JSON 解析失败'); }
      data.mode = document.getElementById('import-replace').checked ? 'replace' : 'merge';
      msg.textContent = '导入中…';
      api('POST', '/api/import', data).then(function (r) {
        if (!r.ok) { msg.textContent = ''; return toast(r.error || '导入失败'); }
        msg.textContent = '导入完成：新增 ' + r.data.categories + ' 个分类、' + r.data.tags + ' 个标签，跳过 ' + r.data.skipped + ' 条。';
        toast('导入完成');
      });
    };
    reader.readAsText(file);
  });

  var bookmarkBtn = document.getElementById('bookmark-btn');
  bookmarkBtn && bookmarkBtn.addEventListener('click', function () {
    var file = document.getElementById('bookmark-file').files[0];
    var msg = document.getElementById('bookmark-msg');
    if (!file) return toast('先选一个书签 HTML 文件');
    var reader = new FileReader();
    reader.onload = function () {
      msg.textContent = '导入中…';
      fetch('/api/import/bookmarks', {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'text/html' },
        body: String(reader.result)
      }).then(function (r) { return r.json(); }).then(function (j) {
        if (!j.ok) { msg.textContent = ''; return toast(j.error || '导入失败'); }
        msg.textContent = '导入完成：' + j.data.categories + ' 个分类、' + j.data.tags + ' 个标签，跳过 ' + j.data.skipped + ' 条。';
        toast('书签导入完成');
      });
    };
    reader.readAsText(file);
  });

  // ---------------- token ----------------
  function renderTokens() {
    var list = document.getElementById('token-list');
    if (!list) return;
    list.textContent = '';
    state.tokens.forEach(function (t) {
      var li = el('li', {}, [
        el('span', { class: 'grow' }, [
          el('strong', { text: t.name }),
          el('span', { class: 'muted small', text: ' · ' + t.scope + (t.last_used ? ' · 最近使用 ' + new Date(t.last_used * 1000).toLocaleString() : ' · 未使用') })
        ]),
        el('button', { class: 'mini', type: 'button', text: '删除', onclick: function () {
          api('DELETE', '/api/tokens/' + t.id).then(function (r) {
            if (!r.ok) return toast(r.error || '删除失败');
            state.tokens = state.tokens.filter(function (x) { return x.id !== t.id; });
            renderTokens();
          });
        } })
      ]);
      list.appendChild(li);
    });
    if (!state.tokens.length) list.appendChild(el('li', { class: 'muted', text: '还没有令牌' }));
  }

  var tokenCreate = document.getElementById('token-create');
  tokenCreate && tokenCreate.addEventListener('click', function () {
    var nameInput = document.getElementById('token-name');
    var scope = document.getElementById('token-scope').value;
    var out = document.getElementById('token-out');
    if (!nameInput.value.trim()) return toast('给令牌起个名字');
    api('POST', '/api/tokens', { name: nameInput.value.trim(), scope: scope }).then(function (r) {
      if (!r.ok) return toast(r.error || '创建失败');
      out.textContent = r.data.token;
      out.title = '复制这个令牌';
      navigator.clipboard && navigator.clipboard.writeText(r.data.token).catch(function () {});
      toast('令牌已创建并复制');
      nameInput.value = '';
      api('GET', '/api/tokens').then(function (r2) {
        if (r2.ok) { state.tokens = r2.data.tokens; renderTokens(); }
      });
    });
  });

  // ---------------- 站点设置 ----------------
  var siteForm = document.getElementById('site-form');
  if (siteForm) {
    siteForm.site_title.value = state.settings.site_title || '';
    siteForm.theme.value = state.settings.theme || 'auto';
    siteForm.accent.value = state.settings.accent || '#3b82f6';
    siteForm.search_engine.value = state.settings.search_engine || '';
    siteForm.open_in_new_tab.checked = !!state.settings.open_in_new_tab;
    siteForm.addEventListener('submit', function (e) {
      e.preventDefault();
      var msg = document.getElementById('site-msg');
      api('PUT', '/api/settings', {
        site_title: siteForm.site_title.value,
        theme: siteForm.theme.value,
        accent: siteForm.accent.value,
        search_engine: siteForm.search_engine.value,
        open_in_new_tab: siteForm.open_in_new_tab.checked
      }).then(function (r) {
        if (!r.ok) { msg.textContent = ''; return toast(r.error || '保存失败'); }
        msg.textContent = '已保存';
        toast('设置已保存');
      });
    });
  }

  var accountForm = document.getElementById('account-form');
  accountForm && accountForm.addEventListener('submit', function (e) {
    e.preventDefault();
    var msg = document.getElementById('account-msg');
    api('PUT', '/api/account', {
      username: accountForm.username.value,
      current_password: accountForm.current_password.value,
      new_password: accountForm.new_password.value
    }).then(function (r) {
      if (!r.ok) { msg.textContent = ''; return toast(r.error || '更新失败'); }
      msg.textContent = '已更新';
      accountForm.current_password.value = '';
      accountForm.new_password.value = '';
      toast('账号已更新');
    });
  });

  // 初始渲染
  renderCats();
  renderTokens();
})();