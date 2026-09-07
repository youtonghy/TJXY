import * as THREE from 'three';
import { RoomEnvironment } from 'three/addons/environments/RoomEnvironment.js';
import { createCinema, createCRT, createFilmMaterial, createHandheld, createLaptop, createLCD, createProjector } from './models';
import type { Model } from './models';

import { CHAPTERS, DURATION } from '../cinematicTimeline';
import { getCinematicSceneDetail } from '../cinematicQuality';

export interface EvolutionController {
  play: () => void;
  pause: () => void;
  seek: (seconds: number) => void;
  dispose: () => void;
}
export interface EvolutionOptions {
  reducedMotion: boolean;
  onFrame: (seconds: number, playing: boolean) => void;
  onFailure: () => void;
  onComplete?: () => void;
}

function setUniform(material: THREE.ShaderMaterial, name: string, value: number) {
  const uniform=material.uniforms[name];
  if(uniform) uniform.value=value;
}
function meshResources(object: THREE.Object3D) {
  const mesh=object as THREE.Object3D & { geometry?: THREE.BufferGeometry; material?: THREE.Material | THREE.Material[] };
  return { geometry:mesh.geometry, materials:mesh.material ? (Array.isArray(mesh.material)?mesh.material:[mesh.material]) : [] };
}
const clamp = THREE.MathUtils.clamp;
// Sine ease-in-out for the long camera moves of this explanatory film.
const ease = (a: number, b: number, t: number) => -(Math.cos(Math.PI * clamp((t-a)/(b-a),0,1))-1)/2;
const mix = THREE.MathUtils.lerp;

export function createEvolutionScene(canvas: HTMLCanvasElement, options: EvolutionOptions): EvolutionController {
  const renderer = new THREE.WebGLRenderer({canvas,antialias:true,powerPreference:'high-performance'});
  const geometries = new Set<THREE.BufferGeometry>();
  const materials = new Set<THREE.Material>();
  const textures = new Set<THREE.Texture>();
  const scene = new THREE.Scene();
  let environment: THREE.WebGLRenderTarget | undefined;
  let observer: ResizeObserver | undefined;
  let raf = 0, seconds = 0, lastTime = 0;
  let playing = false, disposed = false, failed = false;
  const isDisposed = () => disposed;
  const collect = () => {
    scene.traverse(object => {
      const resources=meshResources(object);
      if(resources.geometry) geometries.add(resources.geometry);
      for(const mat of resources.materials) {
        materials.add(mat);
        for(const value of Object.values(mat) as unknown[]) {
          if(value instanceof THREE.Texture) textures.add(value as THREE.Texture);
        }
      }
    });
  };
  const dispose = () => {
    if(disposed) return;
    disposed=true; playing=false;
    cancelAnimationFrame(raf);
    observer?.disconnect();
    document.removeEventListener('visibilitychange',visibility);
    canvas.removeEventListener('webglcontextlost',contextLost);
    collect();
    scene.traverse(object => {
      if (object instanceof THREE.DirectionalLight || object instanceof THREE.SpotLight || object instanceof THREE.PointLight) object.shadow.dispose();
    });
    geometries.forEach(g=>{g.dispose();});
    materials.forEach(m=>{m.dispose();});
    textures.forEach(t=>{t.dispose();});
    environment?.dispose();
    renderer.debug.onShaderError=null;
    renderer.dispose();
  };
  const fail = () => {
    if(failed||disposed) return;
    failed=true;
    dispose();
    options.onFailure();
  };
  const contextLost = (event: Event) => {event.preventDefault();fail();};
  const visibility = () => {
    cancelAnimationFrame(raf);
    lastTime=0;
    if(!document.hidden&&playing&&!disposed) raf=requestAnimationFrame(tick);
  };
  let render: () => void;
  const tick = (now: number) => {
    if(disposed||!playing||document.hidden) return;
    if(lastTime) seconds=Math.min(DURATION,seconds+(now-lastTime)/1000);
    lastTime=now;
    if(seconds>=DURATION) playing=false;
    try {render();} catch {fail();return;}
    if(isDisposed()) return;
    options.onFrame(seconds,playing);
    if(playing&&!isDisposed()) raf=requestAnimationFrame(tick);
    else if(!isDisposed()&&seconds>=DURATION) options.onComplete?.();
  };
  try {
    const detail=getCinematicSceneDetail({
      viewportWidth:canvas.clientWidth || window.innerWidth,
      hardwareConcurrency:navigator.hardwareConcurrency,
      deviceMemory:(navigator as Navigator & {deviceMemory?:number}).deviceMemory,
    });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1,detail==='reduced'?1:1.5));
    renderer.outputColorSpace=THREE.SRGBColorSpace;
    renderer.toneMapping=THREE.ACESFilmicToneMapping;
    renderer.toneMappingExposure=1.2;
    renderer.shadowMap.enabled=true;
    renderer.shadowMap.type=THREE.PCFShadowMap;
    scene.background=new THREE.Color(0x090c10);
    scene.fog=new THREE.Fog(0x090c10,16,36);
    const room=new RoomEnvironment();
    const pmrem=new THREE.PMREMGenerator(renderer);
    try { environment=pmrem.fromScene(room,.025); }
    finally {room.dispose();pmrem.dispose();}
    scene.environment=environment.texture;
    scene.environmentIntensity=.85;
    const camera=new THREE.PerspectiveCamera(36,1,.1,70);
    const key=new THREE.DirectionalLight(0xffecd2,4);
    key.position.set(-3,7,6);
    key.castShadow=true;
    key.shadow.mapSize.setScalar(detail==='reduced'?512:1024);
    key.shadow.camera.left=-9; key.shadow.camera.right=9;
    key.shadow.camera.top=8;key.shadow.camera.bottom=-8;
    key.shadow.normalBias=.03;
    key.shadow.bias=-.0004;
    scene.add(key);
    const rim=new THREE.DirectionalLight(0xa2d4ec,3.4);
    rim.position.set(4,3,-5); scene.add(rim);
    const warm=new THREE.PointLight(0xeaa866,32,18,2);
    warm.position.set(-4,1,2);scene.add(warm);
    const ambient=new THREE.HemisphereLight(0xd2dfeb,0x2d2017,1.1);scene.add(ambient);
    const floor=new THREE.Mesh(new THREE.PlaneGeometry(200,200),new THREE.MeshStandardMaterial({color:0x05080b,roughness:.48,metalness:.35,envMapIntensity:.35}));
    floor.rotation.x=-Math.PI/2;floor.position.y=-2.2;floor.receiveShadow=true;scene.add(floor);
    const films=Array.from({length:6},()=>createFilmMaterial());
    films.forEach(f=>materials.add(f));
    const [cinemaFilm,crtFilm,lcdFilm,laptopFilm,tabletFilm,phoneFilm]=films as [THREE.ShaderMaterial,THREE.ShaderMaterial,THREE.ShaderMaterial,THREE.ShaderMaterial,THREE.ShaderMaterial,THREE.ShaderMaterial];
    const projector=createProjector(), cinema=createCinema(cinemaFilm), crt=createCRT(crtFilm), lcd=createLCD(lcdFilm);
    const laptop=createLaptop(laptopFilm),tablet=createHandheld(tabletFilm,false),phone=createHandheld(phoneFilm,true);
    const models=[projector,cinema,crt,lcd,laptop,tablet,phone];
    models.forEach(model=>{scene.add(model.root);});
    setUniform(crtFilm,'uCrt',1);
    setUniform(phoneFilm,'uAspect',.5);
    setUniform(tabletFilm,'uAspect',.8);
    const entries=new Map<Model, {material:THREE.Material;opacity:number}[]>();
    for (const model of models) {
      const set=new Set<THREE.Material>();
      model.root.traverse(obj=>{
        for(const mat of meshResources(obj).materials) set.add(mat);
      });
      entries.set(model,[...set].map(material=>({material,opacity:material.opacity})));
    }
    const fade=(model:Model,amount:number) => {
      model.root.visible=amount>.001;
      for(const {material,opacity} of entries.get(model)??[]) {
        material.transparent=amount<.999||opacity<1;
        material.opacity=opacity*amount;
        material.depthWrite=amount>.9&&opacity>.9;
        if(material instanceof THREE.ShaderMaterial) setUniform(material,'uOpacity',amount);
      }
    };
    const beamMaterial=new THREE.MeshBasicMaterial({color:0xffd699,transparent:true,opacity:.045,side:THREE.DoubleSide,depthWrite:false,blending:THREE.AdditiveBlending});
    const beam=new THREE.Mesh(new THREE.ConeGeometry(1.05,4,48,1,true),beamMaterial);
    beam.rotation.z=Math.PI/2;
    beam.position.set(3.35,.16,0);
    projector.root.add(beam);
    // Restrained particles in the projection beam, with a stable seed for seeking.
    const points=new Float32Array(150*3);
    for(let i=0;i<150;i++) {
      const random=(n:number)=>{const x=Math.sin(n*127.1)*43758.5453;return x-Math.floor(x);};
      points[i*3]=1.4+random(i+1)*3.6;
      points[i*3+1]=.16+(random(i+300)-.5)*1.25;
      points[i*3+2]=(random(i+600)-.5)*.9;
    }
    const dustGeometry=new THREE.BufferGeometry();dustGeometry.setAttribute('position',new THREE.BufferAttribute(points,3));
    const dustMaterial=new THREE.PointsMaterial({color:0xffd69c,size:.018,transparent:true,opacity:.35,depthWrite:false});
    const dust=new THREE.Points(dustGeometry,dustMaterial);projector.root.add(dust);

    render=()=>{
      const reduced=options.reducedMotion;
      const chapter=CHAPTERS[Math.max(0,CHAPTERS.filter(c=>seconds>=c.time).length-1)];
      const t=reduced?(chapter?.time??0)+3:seconds;
      const toScreen=ease(3.9,6.1,t),toColor=ease(8.7,10.6,t),toCrt=ease(12.25,13.8,t),toLcd=ease(18.25,19.8,t),toDevices=ease(23.3,25.4,t);
      const movementTime=reduced?0:t;
      const heroTurn=reduced?0:Math.sin(movementTime*.22)*.08;
      fade(projector,1-ease(7.6,9,t));
      projector.root.position.set(mix(-.35,-3.4,toScreen),mix(-.2,-.85,toScreen),mix(0,.3,toScreen));
      projector.root.scale.setScalar(mix(1.35,.63,toScreen));
      projector.root.rotation.set(0,-.25+heroTurn,0);
      projector.reels?.forEach((reel,i)=>{reel.rotation.z=movementTime*(i===0?-.85:.73);});
      beamMaterial.opacity=.024*(1-ease(7,9,t));
      dustMaterial.opacity=.32*(1-ease(7,9,t));
      dust.rotation.x=Math.sin(movementTime*.2)*.06;
      fade(cinema,ease(4.15,5.8,t)*(1-toCrt));
      cinema.root.position.set(mix(1.5,0,ease(6.4,9,t)),0,mix(-1.2,0,toColor));
      cinema.root.rotation.y=mix(-.13,.03,toColor)+heroTurn*.3;
      cinema.root.scale.setScalar(mix(.84,1.02,toColor));
      setUniform(cinemaFilm,'uColor',toColor);
      fade(crt,toCrt*(1-toLcd));
      crt.root.position.set(mix(.8,0,toCrt),-.18,0);
      crt.root.rotation.y=mix(-.65,-.22,toCrt)+heroTurn;
      fade(lcd,toLcd*(1-toDevices));
      lcd.root.position.set(0,0,0);
      lcd.root.rotation.y=mix(.75,-.04,toLcd)+heroTurn;
      lcd.root.scale.setScalar(mix(.93,1,toLcd));
      for(const film of films) {
        setUniform(film,'uTime',reduced?10:t);
        if(film!==cinemaFilm) setUniform(film,'uColor',1);
      }
      const spread=ease(24,27.8,t);
      fade(laptop,toDevices);
      laptop.root.position.set(-.75,-.32,-.2);
      laptop.root.rotation.y=-.10+heroTurn*.35;
      laptop.root.scale.setScalar(1.1);
      fade(tablet,toDevices*ease(24,25.6,t));
      tablet.root.position.set(mix(2.85,2.6,spread),-.12,-.95);
      tablet.root.rotation.set(-.04,mix(-.4,-.22,spread),-.08);
      tablet.root.scale.setScalar(.90);
      fade(phone,toDevices*ease(24.4,26,t));
      phone.root.position.set(mix(-3.1,-2.85,spread),-.66,2.25);
      phone.root.rotation.set(.015,.14,.045);
      phone.root.scale.setScalar(.98);
      const portrait=camera.aspect<.85;
      const distance=portrait?(mix(10,11.7,toScreen)+toDevices*2.8)/camera.aspect:11.8;
      const cameraX=mix(3.5,1.3,toScreen)+toCrt*(1-toLcd)*1.65-toDevices*.8;
      camera.position.set(cameraX,2.15,distance);
      camera.lookAt(0,.15,0);
      if (scene.fog instanceof THREE.Fog) {
        scene.fog.near=Math.max(16,distance+4);
        scene.fog.far=scene.fog.near+20;
      }
      warm.intensity=mix(32,10,toDevices);
      renderer.render(scene,camera);
    };
    const resize=()=>{
      if(disposed) return;
      const {width,height}=canvas.getBoundingClientRect();
      renderer.setSize(Math.max(1,width),Math.max(1,height),false);
      camera.aspect=Math.max(1,width)/Math.max(1,height);
      camera.updateProjectionMatrix();
      try {render();} catch {fail();}
    };
    observer=new ResizeObserver(resize);observer.observe(canvas);
    document.addEventListener('visibilitychange',visibility);
    canvas.addEventListener('webglcontextlost',contextLost);
    renderer.debug.onShaderError=fail;
    resize();
    options.onFrame(seconds,false);
    return {
      play:()=>{
        if(disposed||playing) return;
        if(seconds>=DURATION) seconds=0;
        playing=true;lastTime=0;
        if(!document.hidden) raf=requestAnimationFrame(tick);
        options.onFrame(seconds,true);
      },
      pause:()=>{playing=false;lastTime=0;cancelAnimationFrame(raf);options.onFrame(seconds,false);},
      seek:(value:number)=>{
        if(disposed||!Number.isFinite(value)) return;
        seconds=clamp(value,0,DURATION);lastTime=0;
        if(seconds===DURATION) {playing=false;cancelAnimationFrame(raf);}
        try {render();} catch {fail();return;}
        options.onFrame(seconds,playing);
      },
      dispose,
    };
  } catch(error) {
    dispose();
    throw error;
  }
}
