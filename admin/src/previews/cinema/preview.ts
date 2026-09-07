import './style.css';
import { CHAPTERS, DURATION } from '../../setup/cinematicTimeline';
import { createEvolutionScene } from '../../setup/cinematic/scene';
import type { EvolutionController } from '../../setup/cinematic/scene';

const app=document.querySelector<HTMLElement>('#app');
if(!app) throw new Error('Preview root is missing');
const playIcon='<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M8 5.4a.8.8 0 0 1 1.2-.7l10 6.6a.8.8 0 0 1 0 1.4l-10 6.6a.8.8 0 0 1-1.2-.7z"/></svg>';
const pauseIcon='<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><rect x="7" y="5" width="3" height="14" rx="1"/><rect x="14" y="5" width="3" height="14" rx="1"/></svg>';
app.innerHTML=`
  <header class="header">
    <a class="wordmark" href="/cinema-preview.html" aria-label="TJXY 动画预览首页"><span class="brand-mark">◉</span> TJXY<span class="header-divider"></span><span class="header-description">光影的旅程</span></a>
    <span class="preview-badge"><i></i> 独立预览 <span class="badge-version"> / 01</span></span>
  </header>
  <section class="cinematic" aria-label="光影演变 3D 动画">
    <div class="chapter-meta"><span id="chapter-number">01</span><span class="meta-line"></span><span id="chapter-era">THE BEGINNING</span></div>
    <canvas id="scene" aria-label="三维胶卷放映机、电影与现代设备演变"></canvas>
    <div class="vignette" aria-hidden="true"></div>
    <div class="side-note" aria-hidden="true">A JOURNEY THROUGH MOVING IMAGES</div>
    <div class="caption" id="caption"><h1 id="title">光，从这里开始。</h1><p id="subtitle">一束光，两只胶卷盘。让静止的瞬间，第一次流动起来。</p></div>
    <div class="error" id="error" role="alert" hidden><span>◉</span><h2>暂时无法呈现 3D 画面</h2><p>请在支持 WebGL 的浏览器中打开此预览。</p><button id="reload">重新加载</button></div>
  </section>
  <footer class="controls">
    <nav class="chapters" aria-label="动画章节">${CHAPTERS.map((c,i)=>`<button class="chapter ${i===0?'active':''}" data-chapter="${String(i)}" aria-label="预览${c.label}"><span class="chapter-track"><span></span></span><span class="chapter-label"><small>0${String(i+1)}</small>${c.label}</span></button>`).join('')}</nav>
    <div class="transport">
      <div class="playback"><button id="play" class="play" aria-label="播放动画">${playIcon}</button><button id="replay" class="icon-button" aria-label="重新播放"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="M5 8a8 8 0 1 1-1 7M5 3v5h5"/></svg></button><span class="time"><span id="elapsed">00:00</span><span class="time-divider"> / </span>00:32</span></div>
      <label class="scrubber-label"><span class="sr-only">动画进度</span><input id="scrubber" type="range" min="0" max="32" step="0.05" value="0" aria-label="动画进度" /></label>
      <div class="view-controls"><span class="preview-note">已接入 setup</span><button id="fullscreen" class="icon-button" aria-label="全屏预览"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="M9 4H4v5m11-5h5v5M4 15v5h5m11-5v5h-5"/></svg></button></div>
    </div>
  </footer>
`;
function element(selector: string) {
  const target=document.querySelector<HTMLElement>(selector);
  if(!target) throw new Error(`Missing preview element: ${selector}`);
  return target;
}
const play=element('#play') as HTMLButtonElement, scrubber=element('#scrubber') as HTMLInputElement;
const title=element('#title'),subtitle=element('#subtitle'),era=element('#chapter-era'),number=element('#chapter-number');
const elapsed=element('#elapsed');
const chapters=[...document.querySelectorAll<HTMLButtonElement>('[data-chapter]')];
const progress=chapters.map(c=>c.querySelector<HTMLElement>('.chapter-track > span'));
const motion=window.matchMedia('(prefers-reduced-motion: reduce)');
let controller:EvolutionController | undefined;
let playing=false,activeChapter=-1,finished=false,currentTime=0;
function onFrame(seconds:number,isPlaying:boolean) {
  currentTime=seconds;
  if(playing!==isPlaying) {
    playing=isPlaying;
    play.innerHTML=playing?pauseIcon:playIcon;
    play.setAttribute('aria-label',playing?'暂停动画':'播放动画');
  }
  scrubber.value=String(seconds);
  scrubber.setAttribute('aria-valuetext',`${seconds.toFixed(1)} 秒，共 32 秒`);
  elapsed.textContent=`00:${String(Math.floor(seconds)).padStart(2,'0')}`;
  const index=Math.max(0,CHAPTERS.filter(c=>seconds>=c.time).length-1);
  const isFinished=seconds>=29.5;
  if(activeChapter!==index||isFinished!==finished) {
    activeChapter=index;finished=isFinished;
    const chapter=CHAPTERS[index];
    if(chapter) {
      title.textContent=finished?'每一块屏幕，都是新的开始。':chapter.title;
      subtitle.textContent=finished?'TJXY · 让你珍藏的故事，随时随地继续。':chapter.subtitle;
      era.textContent=finished?'THE STORY CONTINUES':chapter.year;
      number.textContent=`0${String(index+1)}`;
    }
    chapters.forEach((button,i)=>{
      button.classList.toggle('active',i===index);
      button.setAttribute('aria-current',i===index?'step':'false');
    });
  }
  CHAPTERS.forEach((chapter,i)=>{
    const end=CHAPTERS[i+1]?.time??DURATION;
    const amount=Math.min(1,Math.max(0,(seconds-chapter.time)/(end-chapter.time)));
    const bar=progress[i];if(bar) bar.style.transform=`scaleX(${String(amount)})`;
  });
}
function start() {
  controller?.dispose();
  try {
    controller=createEvolutionScene(element('#scene') as HTMLCanvasElement,{
      reducedMotion:motion.matches,onFrame,
      onFailure:()=>{element('#error').hidden=false;play.disabled=true;scrubber.disabled=true;},
    });
    // Respect system preferences; chapter navigation remains available.
    if(!motion.matches) controller.play();
  } catch {
    element('#error').hidden=false;play.disabled=true;scrubber.disabled=true;
  }
}
play.addEventListener('click',()=>{if(playing) controller?.pause();else controller?.play();});
element('#replay').addEventListener('click',()=>{controller?.seek(0);controller?.play();});
scrubber.addEventListener('input',()=>{
  const requestedTime=Number(scrubber.value);
  controller?.pause();
  controller?.seek(requestedTime);
});
chapters.forEach(button=>{button.addEventListener('click',()=>{
  const chapter=CHAPTERS[Number(button.dataset.chapter)];
  if(chapter) {controller?.pause();controller?.seek(chapter.time+(chapter.time?1.9:.7));}
});});
element('#reload').addEventListener('click',()=>{window.location.reload();});
element('#fullscreen').addEventListener('click',()=>{
  const toggle=async()=>{
    try {if(document.fullscreenElement) await document.exitFullscreen();else await document.documentElement.requestFullscreen();}
    catch {element('#fullscreen').setAttribute('aria-label','浏览器暂不支持全屏');}
  };void toggle();
});
document.addEventListener('keydown',event=>{
  if(event.target instanceof HTMLInputElement||event.target instanceof HTMLButtonElement||event.target instanceof HTMLAnchorElement) return;
  if(event.code==='Space') {event.preventDefault();play.click();}
  if(event.code==='ArrowRight'||event.code==='ArrowLeft') {event.preventDefault();controller?.pause();controller?.seek(currentTime+(event.code==='ArrowRight'?1:-1));}
});
motion.addEventListener('change',start);
window.addEventListener('pagehide',()=>{controller?.dispose();});
window.addEventListener('pageshow',event=>{if(event.persisted) start();});
if(import.meta.hot) import.meta.hot.dispose(()=>{controller?.dispose();motion.removeEventListener('change',start);});
start();
