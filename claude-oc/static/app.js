const $ = id => document.getElementById(id);
const escapeHtml = value => String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
let timer;
async function loadProfile() {
  try {
    const r = await fetch('/api/profile?_=' + Date.now(), {cache: 'no-store'});
    if (!r.ok) throw new Error('Profile request failed');
    const p = await r.json();
    $('profile').classList.remove('skeleton');
    $('profile').innerHTML = `<img class="avatar" src="${escapeHtml(p.avatar_url)}" alt="${escapeHtml(p.login)} avatar"><div><h1>${escapeHtml(p.name || p.login)}</h1><p>${escapeHtml(p.bio || 'GitHub developer')}</p><div class="stats"><span>${p.public_repos} repos</span><span>${p.followers} followers</span><a href="${escapeHtml(p.html_url)}" target="_blank" rel="noreferrer">@${escapeHtml(p.login)}</a></div></div>`;
  } catch (_) { $('profile').innerHTML = '<p class="error">Profile is temporarily unavailable.</p>'; }
}
async function load() {
  try {
    const params = new URLSearchParams({q: $('q').value, language: $('language').value, sort: $('sort').value});
    params.set('_', Date.now().toString());
    const r = await fetch('/api/repos?' + params, {cache: 'no-store'});
    if (!r.ok) throw new Error('Repositories request failed');
    const data = await r.json();
    const languages = [...new Set(data.map(x => x.language).filter(Boolean))].sort();
    const selected = $('language').value;
    $('language').innerHTML = '<option value="">All languages</option>' + languages.map(x => `<option ${x===selected?'selected':''} value="${escapeHtml(x)}">${escapeHtml(x)}</option>`).join('');
    $('repos').innerHTML = data.length ? data.map(x => `<article><h2><a href="${escapeHtml(x.html_url)}" target="_blank" rel="noreferrer">${x.pinned?'★ ':''}${escapeHtml(x.name)}</a></h2><p>${escapeHtml(x.description || 'No description')}</p><small>★ ${x.stargazers_count} · ${escapeHtml(x.language || 'Other')} · ${escapeHtml((x.topics || []).join(', '))}</small></article>`).join('') : '<p class="empty">No repositories match these filters.</p>';
    $('status').textContent = `${data.length} repositories · updated ${new Date().toLocaleTimeString()}`;
     const lr = await fetch('/api/languages'); const langs = await lr.json(); const canvas = $('languages'), ctx = canvas.getContext('2d'); ctx.clearRect(0,0,600,240); const values = Object.entries(langs); const max = Math.max(1,...values.map(([,v])=>v)); values.forEach(([name,value],i)=>{const x=i*100+10,h=value/max*190;ctx.fillStyle='#58a6ff';ctx.fillRect(x,220-h,70,h);ctx.fillStyle='currentColor';ctx.fillText(name.slice(0,12),x,238);ctx.fillText(value,x+25,215-h);});
  } catch (error) { console.error('Repository loading failed:', error); $('status').textContent = 'Unable to load repositories.'; $('repos').innerHTML = `<p class="error">Unable to load repositories: ${escapeHtml(error.message)}</p>`; }
  clearTimeout(timer); timer = setTimeout(load, 60000);
}
$('q').oninput = load; $('language').onchange = load; $('sort').onchange = load; $('load').onclick = load;
$('theme').onclick = () => { const next = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark'; document.documentElement.dataset.theme = next; localStorage.setItem('theme', next); };
const saved = localStorage.getItem('theme'); if (saved) document.documentElement.dataset.theme = saved;
loadProfile(); load();