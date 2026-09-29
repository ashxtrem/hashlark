/* Self-contained interaction prototype. Fixtures are illustrative, never live results. */
const paths = {
 search:'<circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 4.5 4.5"/>',
 star:'<path d="m12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2-5.6-3-5.6 3 1.1-6.2L3 9.6l6.2-.9z"/>',
 history:'<path d="M3 11a9 9 0 1 1 2.6 7M3 4v7h7m2-5v6l4 2"/>',
 providers:'<rect x="3" y="3" width="6" height="6" rx="1.5"/><rect x="15" y="3" width="6" height="6" rx="1.5"/><rect x="3" y="15" width="6" height="6" rx="1.5"/><rect x="15" y="15" width="6" height="6" rx="1.5"/>',
 settings:'<path d="m9 3-.5 3-2 1-3-.5-1 4L5 12l-.2 2.3L3 16l2 3 3-1 2 1 1 3 4-1 .4-3 2-1 3 .5 1-4-2.5-1.5.2-2.3L21 7l-2-3-3 1-2-1-1-2z"/><circle cx="12" cy="12" r="3"/>',
 shield:'<path d="m12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6z"/><path d="m8.5 12 2.3 2.3 4.7-5"/>',
 close:'<path d="m6 6 12 12M6 18 18 6"/>',
 'arrow-right':'<path d="M4 12h16m-6-6 6 6-6 6"/>',
 'arrow-left':'<path d="M20 12H4m6-6-6 6 6 6"/>',
 'chevron-right':'<path d="m9 5 7 7-7 7"/>',
 sliders:'<path d="M4 6h4m4 0h8M4 12h10m4 0h2M4 18h2m4 0h10M8 3v6m6 0v6M6 15v6"/>',
 sort:'<path d="M4 5h16M4 12h11M4 19h6"/>',
 code:'<path d="m8 7-5 5 5 5m8-10 5 5-5 5m-3-12-2 14"/>',
 up:'<path d="M12 20V4m-5 5 5-5 5 5"/>',
 check:'<path d="m5 12 4 4L19 6"/>',
 external:'<path d="M14 3h7v7m0-7L10 14m-1-9H4v15h15v-5"/>',
 copy:'<rect x="8" y="8" width="12" height="13" rx="2"/><path d="M15 8V3H3v12h5"/>',
 magnet:'<path d="M5 10V4h5v9a2 2 0 0 0 4 0V4h5v9a7 7 0 0 1-14 0zM5 8h5m4 0h5"/>',
 globe:'<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c6 6 6 12 0 18-6-6-6-12 0-18"/>',
 clock:'<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>'
};
const icon = name => `<svg class="icon" viewBox="0 0 24 24" aria-hidden="true">${paths[name] || paths.code}</svg>`;
const esc = value => String(value).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const $ = selector => document.querySelector(selector);
const providers = ['Internet Archive','LinuxTracker','Academic Torrents','FOSS Torrents'];
const fixtures = [
 {id:1,title:'LibreELEC-AMLGX.aarch64-12.2.0-khadas-vim3.img.gz',size:149.3,seeds:18,peers:3,age:'1 year ago',category:'Software',sources:[0,1],published:'12 September 2025',type:'Linux disk image',format:'.img.gz',match:100},
 {id:2,title:'vim-9.1.tar.bz2',size:17.8,seeds:124,peers:12,age:'8 months ago',category:'Software',sources:[0,3],published:'22 January 2026',type:'Source code archive',format:'.tar.bz2',match:99},
 {id:3,title:'LibreELEC-AMLGX.aarch64-12.2.0-khadas-vim2.img.gz',size:149.6,seeds:8,peers:1,age:'1 year ago',category:'Software',sources:[0],published:'12 September 2025',type:'Linux disk image',format:'.img.gz',match:98},
 {id:4,title:'github.com-thosakwe-vim-flutter_-_2023-06-07_10-03-15',size:737.7/1024,seeds:null,peers:null,age:'3 years ago',category:'Software',sources:[0],published:'7 June 2023',type:'Source code archive',format:'.zip',match:97},
 {id:5,title:'LibreELEC-AMLGX.aarch64-12.2.0-khadas-vim.img.gz',size:149.6,seeds:1,peers:0,age:'1 year ago',category:'Software',sources:[0],published:'12 September 2025',type:'Linux disk image',format:'.img.gz',match:96},
 {id:6,title:'LibreELEC-AMLGX.aarch64-12.2.0-khadas-vim3l.img.gz',size:149.3,seeds:0,peers:0,age:'1 year ago',category:'Software',sources:[0],published:'12 September 2025',type:'Linux disk image',format:'.img.gz',match:95},
 {id:7,title:'Vim: an introduction to modal editing',size:86.4,seeds:32,peers:4,age:'6 months ago',category:'Books',sources:[0],published:'24 March 2026',type:'Learning resource',format:'.pdf',match:94},
 {id:8,title:'VimConf — community talks and workshops',size:842,seeds:16,peers:2,age:'2 years ago',category:'Video',sources:[0],published:'23 November 2024',type:'Conference recordings',format:'.mp4',match:93},
 {id:9,title:'ubuntu-24.04.3-desktop-amd64.iso',size:6200,seeds:482,peers:21,age:'1 year ago',category:'Software',sources:[1,3],published:'7 August 2025',type:'Linux disk image',format:'.iso',match:92}
];
const sections = [['Search','search'],['Favourites','star'],['History','history'],['Providers','providers'],['Settings','settings']];
const categories = ['All','Movies','TV','Music','Software','Books','Video'];
let saved;
try { saved = new Set(JSON.parse(localStorage.getItem('hashlark-ui-study-saved') || '[2]')); } catch { saved = new Set([2]); }
const state = {page:'Search',query:'vim',category:'All',sort:'relevance',seeded:false,providers:[0,1,2,3],selected:null,history:['vim','ubuntu'],scroll:0};
let toastTimer;
const wide = () => matchMedia('(min-width:840px) and (min-height:480px)').matches;
function hydrate(root=document){root.querySelectorAll('[data-icon]').forEach(el=>el.innerHTML=icon(el.dataset.icon));}
function sizeLabel(item){return item.size<1?`${(item.size*1024).toFixed(1)} KiB`:item.size>=1024?`${(item.size/1024).toFixed(1)} GiB`:`${item.size.toFixed(1)} MiB`;}
function toast(message){$('#toast').textContent=message;$('#toast').hidden=false;clearTimeout(toastTimer);toastTimer=setTimeout(()=>$('#toast').hidden=true,3500);}
function renderNavigation(){
 const html=sections.map(([label,symbol])=>`<button class="nav-button ${state.page===label?'active':''}" data-page="${label}" data-short="${label==='Favourites'?'Saved':label}" ${state.page===label?'aria-current="page"':''}>${icon(symbol)}<span class="nav-label">${label}</span>${label==='Favourites'?`<span class="count">${saved.size}</span>`:''}</button>`).join('');
 $('.navigation').innerHTML=html;$('.bottom-navigation').innerHTML=html;
}
function filtered(){
 let items=fixtures.filter(x=>(state.page==='Favourites'?saved.has(x.id):x.title.toLowerCase().includes(state.query.toLowerCase())) && (state.category==='All'||x.category===state.category) && (!state.seeded||x.seeds>0) && x.sources.some(s=>state.providers.includes(s)));
 return items.sort((a,b)=>state.sort==='seeders'?(b.seeds??-1)-(a.seeds??-1):state.sort==='size'?a.size-b.size:state.sort==='title'?a.title.localeCompare(b.title):b.match-a.match);
}
function renderCategories(){ $('.categories').innerHTML=categories.map(c=>`<button class="category ${c===state.category?'active':''}" data-category="${c}" aria-pressed="${c===state.category}">${c}</button>`).join(''); }
function renderResults(){
 const items=filtered();
 $('#results-title').textContent=state.page==='Favourites'?'Your saved results':'Search results';
 $('#results-summary').textContent=state.page==='Favourites'?`${items.length} saved ${items.length===1?'result':'results'}`:`${items.length} ${items.length===1?'result':'results'}${state.query?' for “'+state.query+'”':''} · Sample data`;
 $('#list-caption').textContent=state.category==='All'?'ALL RESULTS':state.category.toUpperCase();
 $('#provider-status-text').textContent=`${state.providers.length} providers`;
 $('#filter-count').hidden=!state.seeded&&state.providers.length===4;
 $('#filter-count').textContent=Number(state.seeded)+Number(state.providers.length!==4);
 $('#results').innerHTML=items.length?items.map(x=>`<article class="result-row ${state.selected===x.id?'selected':''}" data-id="${x.id}"><button class="result-open" data-open="${x.id}" aria-label="View ${esc(x.title)}" ${state.selected===x.id?'aria-current="true"':''}><span class="file-icon">${icon(x.category==='Software'?'code':'globe')}</span><span class="result-copy"><span class="result-name">${esc(x.title)}</span><span class="result-meta"><span>${sizeLabel(x)}</span><span class="seeds ${x.seeds===null?'unknown':''}" aria-label="${x.seeds===null?'Seed count not reported':x.seeds+(x.seeds===1?' seeder':' seeders')}">${icon('up')}${x.seeds===null?'Unknown':x.seeds+(x.seeds===1?' seed':' seeds')}</span><span>${x.sources.length} ${x.sources.length===1?'source':'sources'}</span><span class="age">${x.age}</span></span><span class="result-source">${x.type} · ${providers[x.sources[0]]}</span></span></button><button class="row-action ${saved.has(x.id)?'saved':''}" data-save="${x.id}" aria-label="${saved.has(x.id)?'Unsave':'Save'} ${esc(x.title)}" aria-pressed="${saved.has(x.id)}">${icon('star')}</button></article>`).join(''):`<div class="empty"><span class="file-icon">${icon(state.page==='Favourites'?'star':'search')}</span><h3>${state.page==='Favourites'?'Nothing saved here yet':'No matching results'}</h3><p>${state.page==='Favourites'?'Tap the star on a result to keep it here.':'Try “vim” or “ubuntu”, or broaden your filters. This preview searches a small set of sample results.'}</p><button class="secondary" data-reset-search>${state.page==='Favourites'?'Explore results':'Reset search'}</button></div>`;
 $('#list-footer-text').textContent=state.page==='Favourites'?'Saved on this device for this prototype':'Duplicate results are combined across providers';
 if(state.selected&&!items.some(x=>x.id===state.selected)){closeDetail(false);}
 $('#workspace').classList.toggle('has-selection',Boolean(state.selected));
}
function renderDetail(){
 const x=fixtures.find(x=>x.id===state.selected);if(!x)return;
 $('#detail').hidden=false;
 $('#detail').innerHTML=`<div class="detail-topbar"><button class="icon-button detail-back" data-close-detail aria-label="Back to results">${icon('arrow-left')}</button><span class="eyebrow">RESULT DETAILS</span><button class="icon-button detail-close" data-close-detail aria-label="Close details">${icon('close')}</button></div><div class="detail-content"><div class="detail-hero">${icon('code')}</div><p class="detail-category">${x.category} <span aria-hidden="true">/</span> ${x.type}</p><h2 class="detail-title" id="detail-title" tabindex="-1">${esc(x.title)}</h2><p class="detail-subtitle">Published ${x.published}<br>Found on ${x.sources.length} independent ${x.sources.length===1?'provider':'providers'}</p><div class="detail-actions"><button class="primary" data-preview-action="Open in client">${icon('magnet')}Open in client${icon('external')}</button><button class="secondary ${saved.has(x.id)?'saved':''}" data-save="${x.id}" aria-label="${saved.has(x.id)?'Unsave':'Save'} result" aria-pressed="${saved.has(x.id)}">${icon('star')}</button></div><button class="copy-action" data-preview-action="Copy magnet">${icon('copy')}Copy magnet link</button><div class="facts-grid"><div><div class="fact-label">File size</div><div class="fact-value">${sizeLabel(x)}</div></div><div><div class="fact-label">File format</div><div class="fact-value">${x.format}</div></div><div><div class="fact-label">Seeders</div><div class="fact-value green">${x.seeds??'—'}<small>${x.seeds===null?'Not reported':x.seeds===0?'None reported':'available'}</small></div></div><div><div class="fact-label">Peers</div><div class="fact-value">${x.peers??'—'}<small>${x.peers===null?'Not reported':'connected'}</small></div></div></div><h3 class="detail-section-title">Found on<span>${x.sources.length} ${x.sources.length===1?'source':'sources'}</span></h3>${x.sources.map(i=>`<div class="source-card"><span class="source-mark">${providers[i].charAt(0)}</span><span class="source-name">${providers[i]}<small>Provider result</small></span><button class="icon-button" data-preview-action="View on ${providers[i]}" aria-label="View on ${providers[i]}">${icon('external')}</button></div>`).join('')}<details class="hash-block"><summary>Technical details</summary><p>Infohash: not included in this sample.</p><p>Original filename: <code>${esc(x.title)}</code></p></details><p class="detail-note">Hashlark finds the result. Your torrent client handles the download.</p></div>`;
 syncDetailMode();
}
function syncDetailMode(){
 const modal=Boolean(state.selected)&&!wide();
 document.body.classList.toggle('detail-open',modal);
 $('.sidebar').inert=modal;$('.bottom-navigation').inert=modal;
 document.querySelectorAll('.page-heading,#search-controls,.results-toolbar,.result-panel').forEach(el=>el.inert=modal);
 if(modal){$('#detail').setAttribute('role','dialog');$('#detail').setAttribute('aria-modal','true');$('#detail').setAttribute('aria-labelledby','detail-title');}
 else{$('#detail').removeAttribute('role');$('#detail').removeAttribute('aria-modal');$('#detail').removeAttribute('aria-labelledby');}
}
function openDetail(id){
 state.scroll=window.scrollY;state.selected=id;renderResults();renderDetail();
 history.pushState({hashlarkDetail:true},'',location.pathname+'?selected='+id);
 $('#detail-title').focus({preventScroll:true});
}
function closeDetail(restore=true){
 const old=state.selected;state.selected=null;$('#detail').hidden=true;$('#workspace').classList.remove('has-selection');syncDetailMode();
 document.querySelectorAll('.result-row.selected').forEach(el=>el.classList.remove('selected'));
 document.querySelectorAll('.result-open[aria-current]').forEach(el=>el.removeAttribute('aria-current'));
 history.replaceState(null,'',location.pathname);
 if(restore){window.scrollTo(0,state.scroll);document.querySelector(`[data-open="${old}"]`)?.focus({preventScroll:true});}
}
function changePage(page){
 closeDetail(false);state.page=page;state.category='All';renderNavigation();renderCategories();
 $('#page-title').innerHTML=esc(page)+'<span class="heading-dot">.</span>';
 $('#search-controls').hidden=page!=='Search';$('#search-page').hidden=!['Search','Favourites'].includes(page);$('#other-page').hidden=['Search','Favourites'].includes(page);
 if(['Search','Favourites'].includes(page)){renderResults();}
 else if(page==='Providers')renderProviders();
 else if(page==='History')renderHistory();
 else renderSettings();
 window.scrollTo(0,0);
}
function renderProviders(){
 $('#other-page').innerHTML=`<p class="section-intro">Choose where Hashlark looks. Results from the same torrent are merged automatically.</p><div class="simple-panel"><h2>Search providers</h2><p>Illustrative provider connections for this UI study.</p>${providers.map((p,i)=>`<label class="simple-row"><span class="source-mark">${p.charAt(0)}</span><div><h3>${p}</h3><p>${['Open collections & archives','Linux distributions','Research & datasets','Open-source software'][i]}</p></div><input type="checkbox" data-provider-toggle="${i}" ${state.providers.includes(i)?'checked':''} aria-label="Enable ${p}"></label>`).join('')}</div>`;
}
function renderHistory(){
 $('#other-page').innerHTML=`<p class="section-intro">Pick up where you left off.</p><div class="simple-panel"><h2>Recent searches</h2><p>Search history from this preview session.</p>${state.history.map(q=>`<button class="simple-row history-item" data-history="${esc(q)}">${icon('history')}<div><h3>${esc(q)}</h3><p>Search again across your providers</p></div>${icon('arrow-right')}</button>`).join('')}</div>`;
}
function renderSettings(){
 $('#other-page').innerHTML=`<p class="section-intro">A quieter place to find what you need.</p><div class="simple-panel"><h2>About this preview</h2><p>Visual direction for the Android UI refactor.</p><div class="simple-row"><div><h3>Appearance</h3><p>Warm neutral surfaces with forest green accents</p></div><span class="setting-value">Light</span></div><div class="simple-row"><div><h3>Layout</h3><p>Adapts to the available window width</p></div><span class="setting-value">Adaptive</span></div><div class="simple-row"><div><h3>Sample data</h3><p>Provider connections and torrent actions are simulated</p></div><span class="preview-badge">PREVIEW</span></div><div class="simple-row"><div><h3>Refactor reference</h3><p>Compare the phone and expanded layouts</p></div><a class="secondary" href="review.html">Open study ${icon('external')}</a></div></div>`;
}
function showFilters(){
 $('#has-seeders').checked=state.seeded;
 $('#provider-filters').innerHTML=providers.map((p,i)=>`<label class="check-line"><strong>${p}</strong><input type="checkbox" name="provider" value="${i}" ${state.providers.includes(i)?'checked':''}></label>`).join('');
 $('#filter-dialog').showModal();
}
function submitSearch(){
 closeDetail(false);state.query=$('#query').value.trim();state.page='Search';
 if(state.query){state.history=[state.query,...state.history.filter(q=>q!==state.query)].slice(0,8);}
 changePage('Search');$('#query').blur();
}
document.addEventListener('click',event=>{
 const button=event.target.closest('button');if(!button)return;
 if(button.dataset.page)changePage(button.dataset.page);
 else if(button.dataset.category){state.category=button.dataset.category;renderCategories();renderResults();}
 else if(button.dataset.open)openDetail(Number(button.dataset.open));
 else if(button.dataset.save){
  const id=Number(button.dataset.save);saved.has(id)?saved.delete(id):saved.add(id);
  try{localStorage.setItem('hashlark-ui-study-saved',JSON.stringify([...saved]));}catch{}
  const wasDetail=Boolean(button.closest('#detail'));
  renderNavigation();renderResults();if(state.selected)renderDetail();
  document.querySelector(`${wasDetail?'#detail':'#results'} [data-save="${id}"]`)?.focus({preventScroll:true});
  toast(saved.has(id)?'Saved to Favourites':'Removed from Favourites');
 }
 else if(button.hasAttribute('data-close-detail'))closeDetail();
 else if(button.hasAttribute('data-close-dialog'))$('#filter-dialog').close();
 else if(button.dataset.previewAction)toast(`${button.dataset.previewAction} — preview only. No live torrent is attached.`);
 else if(button.hasAttribute('data-reset-search')){state.query='vim';$('#query').value='vim';state.category='All';state.seeded=false;state.providers=[0,1,2,3];changePage('Search');}
 else if(button.dataset.history){$('#query').value=button.dataset.history;submitSearch();}
});
$('#search-form').addEventListener('submit',e=>{e.preventDefault();submitSearch();});
$('.clear-search').addEventListener('click',()=>{$('#query').value='';$('#query').focus();});
$('#sort').addEventListener('change',e=>{state.sort=e.target.value;renderResults();});
$('#filters-button').addEventListener('click',showFilters);
$('#provider-status').addEventListener('click',showFilters);
$('#reset-filters').addEventListener('click',()=>{$('#has-seeders').checked=false;document.querySelectorAll('[name=provider]').forEach(el=>el.checked=true);});
$('#filter-form').addEventListener('submit',e=>{e.preventDefault();state.seeded=$('#has-seeders').checked;state.providers=[...document.querySelectorAll('[name=provider]:checked')].map(el=>Number(el.value));$('#filter-dialog').close();renderResults();});
document.addEventListener('change',e=>{if(e.target.matches('[data-provider-toggle]')){const id=Number(e.target.dataset.providerToggle);state.providers=e.target.checked?[...state.providers,id]:state.providers.filter(p=>p!==id);toast(`${providers[id]} ${e.target.checked?'enabled':'disabled'} in this preview`);}});
document.addEventListener('keydown',e=>{
 if(e.key==='Escape'&&state.selected&&!$('#filter-dialog').open){e.preventDefault();closeDetail();}
 if((e.ctrlKey||e.metaKey)&&e.key==='k'){e.preventDefault();changePage('Search');$('#query').focus();}
 if(e.key==='Tab'&&state.selected&&!wide()){
  const targets=[...$('#detail').querySelectorAll('button,summary,a[href]')].filter(el=>el.getClientRects().length);
  const first=targets[0],last=targets.at(-1);
  if(e.shiftKey&&(document.activeElement===first||document.activeElement===$('#detail-title'))){e.preventDefault();last.focus();}
  else if(!e.shiftKey&&document.activeElement===last){e.preventDefault();first.focus();}
 }
});
window.addEventListener('resize',syncDetailMode);
window.addEventListener('popstate',()=>{
 const id=Number(new URLSearchParams(location.search).get('selected'));
 if(filtered().some(x=>x.id===id)){state.selected=id;renderResults();renderDetail();}
 else closeDetail();
});
hydrate();renderNavigation();renderCategories();renderResults();
const selected=Number(new URLSearchParams(location.search).get('selected'));
if(filtered().some(x=>x.id===selected)){state.selected=selected;renderResults();renderDetail();}
