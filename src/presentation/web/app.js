const $ = selector => document.querySelector(selector);
const key = new URLSearchParams(location.search).get('KEY');
let data = { services: [], proxies: [] };

const escapeHtml = value => String(value).replace(/[&<>'"]/g, char => ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', "'":'&#39;', '"':'&quot;' }[char]));
const api = async (url, options) => {
  const separator = url.includes('?') ? '&' : '?';
  const response = await fetch(`${url}${separator}KEY=${encodeURIComponent(key || '')}`, options);
  if (!response.ok) throw Error(await response.text());
  return response.status === 204 ? null : response.json();
};
function toast(message, error = false) {
  const element = $('#toast'); element.textContent = message; element.className = `show${error ? ' error' : ''}`;
  clearTimeout(toast.timer); toast.timer = setTimeout(() => { element.className = ''; }, 3500);
}
function serviceRow(service) {
  return `<div class="row"><div><b>${escapeHtml(service.id)}</b><div class="meta">PID ${escapeHtml(service.pid)} · ${escapeHtml(service.config)}</div></div><button onclick="removeItem('service','${encodeURIComponent(service.id)}')">Parar</button></div>`;
}
function proxyRow(proxy) {
  return `<div class="row"><div><b>${escapeHtml(proxy.kind)} · ${escapeHtml(proxy.id)}</b><div class="meta">${escapeHtml(proxy.url)}</div></div><button onclick="removeItem('proxy','${encodeURIComponent(proxy.id)}')">Remover</button></div>`;
}
function render() {
  const { services, proxies } = data;
  $('#metric-services').textContent = services.length; $('#metric-proxies').textContent = proxies.length;
  $('#routine-count').textContent = services.length; $('#proxy-count').textContent = proxies.length;
  const serviceMarkup = services.length ? services.map(serviceRow).join('') : 'Nenhuma rotina em execução.';
  $('#services').innerHTML = serviceMarkup; $('#overview-services').innerHTML = serviceMarkup;
  $('#proxies').innerHTML = proxies.length ? proxies.map(proxyRow).join('') : 'Nenhuma fonte cadastrada.';
}
async function refresh() {
  try { data.services = await api('/api/services'); data.proxies = await api('/api/proxies'); render(); $('#connection').textContent = 'Conectado'; }
  catch (error) { $('#connection').textContent = 'Reconectando'; toast(error.message, true); }
}
window.removeItem = async (type, id) => {
  try { await api(`/api/${type === 'service' ? 'services' : 'proxies'}/${id}`, { method: 'DELETE' }); toast(type === 'service' ? 'Rotina encerrada.' : 'Fonte removida.'); refresh(); }
  catch (error) { toast(error.message, true); }
};
function bindForm(selector, endpoint, message) {
  $(selector).addEventListener('submit', async event => {
    event.preventDefault(); const body = Object.fromEntries(new FormData(event.currentTarget));
    if (body.bots) body.bots = Number(body.bots); else delete body.bots;
    try { await api(endpoint, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) }); event.currentTarget.reset(); toast(message); refresh(); }
    catch (error) { toast(error.message, true); }
  });
}
function openTab(name) {
  document.querySelectorAll('.tab').forEach(tab => tab.classList.toggle('active', tab.dataset.tab === name));
  document.querySelectorAll('.panel').forEach(panel => panel.classList.toggle('active', panel.dataset.panel === name));
  $('#page-title').textContent = ({ overview: 'Visão geral', routines: 'Rotinas', proxies: 'Fontes proxy' })[name];
  location.hash = name;
}
document.querySelectorAll('.tab').forEach(tab => tab.addEventListener('click', () => openTab(tab.dataset.tab)));
document.querySelectorAll('.jump').forEach(button => button.addEventListener('click', () => openTab(button.dataset.target)));
$('#refresh').addEventListener('click', refresh); $('#refresh-services').addEventListener('click', refresh); $('#refresh-proxies').addEventListener('click', refresh);
bindForm('#service', '/api/services', 'Rotina iniciada.'); bindForm('#proxy', '/api/proxies', 'Fonte de proxy salva.');
openTab(['overview','routines','proxies'].includes(location.hash.slice(1)) ? location.hash.slice(1) : 'overview'); refresh(); setInterval(refresh, 5000);
