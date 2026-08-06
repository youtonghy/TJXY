import * as THREE from 'three';

import { getCinematicSceneDetail } from './cinematicQuality';
import { createCinematicResourceScope } from './cinematicResources';
import type { DisposableResource } from './cinematicResources';
import { CINEMATIC_DURATION_MS, getCinematicPhaseStartSeconds, getCinematicTimelineFrame } from './cinematicTimeline';
import { createProceduralFilmTexture } from './filmTexture';

export interface CinematicSceneOptions {
  onComplete: () => void;
  onFailure: () => void;
  reducedMotion: boolean;
}

export interface CinematicSceneController {
  dispose: () => void;
  start: () => void;
}

export type CinematicSceneFactory = (
  canvas: HTMLCanvasElement,
  options: CinematicSceneOptions,
) => CinematicSceneController;

interface TrackedMaterial { material: THREE.Material; opacity: number }
interface Visual { materials: TrackedMaterial[]; root: THREE.Group }

interface CinemaSet extends Visual {
  aisleMaterial: THREE.MeshBasicMaterial;
  curtainLeft: THREE.Group;
  curtainMaterial: THREE.MeshStandardMaterial;
  curtainRight: THREE.Group;
  dust: THREE.Points;
  dustMaterial: THREE.PointsMaterial;
  screen: THREE.Mesh;
  screenLight: THREE.PointLight;
  screenMaterial: THREE.MeshBasicMaterial;
  seatMaterial: THREE.MeshStandardMaterial;
  wallMaterial: THREE.MeshStandardMaterial;
}

interface HomeSet extends Visual {
  accentLight: THREE.PointLight;
  lampLight: THREE.PointLight;
}

interface DeviceSet extends Visual {
  filmMaterial: THREE.MeshBasicMaterial;
}

interface PhoneSet extends DeviceSet {
  brandLight: THREE.PointLight;
  brandMaterial: THREE.MeshBasicMaterial;
}

interface CinematicWorld {
  ambient: THREE.HemisphereLight;
  camera: THREE.PerspectiveCamera;
  cinema: CinemaSet;
  film: ReturnType<typeof createProceduralFilmTexture>;
  home: HomeSet;
  phone: PhoneSet;
  scene: THREE.Scene;
  tablet: DeviceSet;
  television: DeviceSet;
}

const REDUCED_MOTION_DURATION_MS = 1_500;
const MONO_SEAT = new THREE.Color(0x171717);
const COLOUR_SEAT = new THREE.Color(0x5b1524);
const MONO_WALL = new THREE.Color(0x17191a);
const COLOUR_WALL = new THREE.Color(0x142e31);
const MONO_CURTAIN = new THREE.Color(0x242424);
const COLOUR_CURTAIN = new THREE.Color(0x771f30);
const CINEMA_BACKGROUND = new THREE.Color(0x020303);
const HOME_BACKGROUND = new THREE.Color(0x171512);
const FINAL_GLOW = new THREE.Color(0x8a6d4d);
const PHASE_START_SECONDS = {
  brand: getCinematicPhaseStartSeconds('brand-handoff'),
  colour: getCinematicPhaseStartSeconds('colour-cinema'),
  film: getCinematicPhaseStartSeconds('monochrome-film'),
  phone: getCinematicPhaseStartSeconds('phone'),
  tablet: getCinematicPhaseStartSeconds('tablet'),
  television: getCinematicPhaseStartSeconds('television'),
};

export const createCinematicScene: CinematicSceneFactory = (canvas, options) => {
  const resources = createCinematicResourceScope();
  try {
    return createCinematicSceneController(canvas, options, resources);
  } catch (error) {
    resources.dispose();
    throw error;
  }
};

function createCinematicSceneController(
  canvas: HTMLCanvasElement,
  options: CinematicSceneOptions,
  resources: ReturnType<typeof createCinematicResourceScope>,
): CinematicSceneController {
  const { track } = resources;
  const renderer = track(new THREE.WebGLRenderer({
    alpha: false,
    antialias: true,
    canvas,
    powerPreference: 'high-performance',
  }));
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.08;
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.5));
  renderer.setClearColor(CINEMA_BACKGROUND, 1);

  const scene = new THREE.Scene();
  scene.background = CINEMA_BACKGROUND.clone();
  scene.fog = new THREE.Fog(CINEMA_BACKGROUND, 8, 34);
  const camera = new THREE.PerspectiveCamera(46, 1, 0.1, 60);
  const ambient = new THREE.HemisphereLight(0xbccbd0, 0x070606, 0.08);
  scene.add(ambient);

  const film = track(createProceduralFilmTexture());
  const brand = track(createBrandTexture());
  const viewportWidth = canvas.clientWidth || window.innerWidth;
  const navigatorWithMemory = navigator as Navigator & { deviceMemory?: number };
  const detail = getCinematicSceneDetail({
    deviceMemory: navigatorWithMemory.deviceMemory,
    hardwareConcurrency: navigator.hardwareConcurrency,
    viewportWidth,
  });
  const cinema = createCinemaSet(track, film.texture, detail === 'reduced');
  const home = createHomeSet(track);
  const television = createTelevision(track, film.texture);
  const tablet = createTablet(track, film.texture);
  const phone = createPhone(track, film.texture, brand.texture);
  scene.add(cinema.root, home.root, television.root, tablet.root, phone.root);
  const world: CinematicWorld = {
    ambient,
    camera,
    cinema,
    film,
    home,
    phone,
    scene,
    tablet,
    television,
  };

  let animationFrame: number | undefined;
  let accumulatedMilliseconds = 0;
  let resumedAt = 0;
  let started = false;
  let running = false;
  let disposed = false;
  let failed = false;

  const resize = () => {
    if (disposed) return;
    const bounds = canvas.getBoundingClientRect();
    const width = Math.max(Math.round(bounds.width || window.innerWidth), 1);
    const height = Math.max(Math.round(bounds.height || window.innerHeight), 1);
    renderer.setSize(width, height, false);
    camera.aspect = width / height;
    camera.fov = camera.aspect < 0.72 ? 53 : 46;
    camera.updateProjectionMatrix();
  };
  const resizeObserver = typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(resize);
  resizeObserver?.observe(canvas);
  window.addEventListener('resize', resize);

  const pause = () => {
    if (!running) return;
    accumulatedMilliseconds += performance.now() - resumedAt;
    running = false;
    if (animationFrame !== undefined) cancelAnimationFrame(animationFrame);
    animationFrame = undefined;
  };

  const fail = () => {
    if (failed || disposed) return;
    failed = true;
    pause();
    queueMicrotask(() => { if (!disposed) options.onFailure(); });
  };
  renderer.debug.onShaderError = () => { fail(); };

  const renderFrame = (now: number) => {
    if (!running || disposed) return;
    const elapsedMilliseconds = accumulatedMilliseconds + now - resumedAt;
    try {
      if (options.reducedMotion) applyReducedMotionFrame(
        Math.min(elapsedMilliseconds / REDUCED_MOTION_DURATION_MS, 1),
        world,
      );
      else applyCinematicFrame(elapsedMilliseconds, world);
      renderer.render(scene, camera);
    } catch {
      fail();
      return;
    }

    const duration = options.reducedMotion ? REDUCED_MOTION_DURATION_MS : CINEMATIC_DURATION_MS;
    if (elapsedMilliseconds >= duration) {
      running = false;
      animationFrame = undefined;
      options.onComplete();
      return;
    }
    animationFrame = requestAnimationFrame(renderFrame);
  };

  const resume = () => {
    if (!started || running || disposed || failed || document.hidden) return;
    resumedAt = performance.now();
    running = true;
    animationFrame = requestAnimationFrame(renderFrame);
  };
  const onVisibilityChange = () => { if (document.hidden) pause(); else resume(); };
  const onContextLost = (event: Event) => { event.preventDefault(); fail(); };
  const onContextCreationError = () => { fail(); };
  document.addEventListener('visibilitychange', onVisibilityChange);
  canvas.addEventListener('webglcontextlost', onContextLost);
  canvas.addEventListener('webglcontextcreationerror', onContextCreationError);

  const dispose = () => {
    if (disposed) return;
    disposed = true;
    running = false;
    if (animationFrame !== undefined) cancelAnimationFrame(animationFrame);
    animationFrame = undefined;
    document.removeEventListener('visibilitychange', onVisibilityChange);
    window.removeEventListener('resize', resize);
    canvas.removeEventListener('webglcontextlost', onContextLost);
    canvas.removeEventListener('webglcontextcreationerror', onContextCreationError);
    resizeObserver?.disconnect();
    renderer.debug.onShaderError = null;
    resources.dispose();
  };

  return {
    dispose,
    start: () => {
      if (started || disposed) return;
      started = true;
      resize();
      if (options.reducedMotion) film.update(9_000, 1);
      else film.update(0, 0);
      resume();
    },
  };
}

function applyCinematicFrame(
  elapsedMilliseconds: number,
  { ambient, camera, cinema, film, home, phone, scene, tablet, television }: CinematicWorld,
) {
  const frame = getCinematicTimelineFrame(elapsedMilliseconds);
  const seconds = frame.elapsedMilliseconds / 1_000;
  const wake = smoothstep(0, 3, seconds);
  const filmStarted = smoothstep(PHASE_START_SECONDS.film - 0.55, PHASE_START_SECONDS.film + 0.15, seconds);
  const colourMix = smoothstep(PHASE_START_SECONDS.colour, PHASE_START_SECONDS.television, seconds);
  const homeMix = smoothstep(PHASE_START_SECONDS.television, PHASE_START_SECONDS.tablet, seconds);
  const tabletMix = smoothstep(PHASE_START_SECONDS.tablet, PHASE_START_SECONDS.phone, seconds);
  const phoneMix = smoothstep(PHASE_START_SECONDS.phone, PHASE_START_SECONDS.brand, seconds);
  const brandMix = smoothstep(PHASE_START_SECONDS.brand, CINEMATIC_DURATION_MS / 1_000, seconds);
  const finalGlow = pulse(seconds, 17.43, 0.2);
  const fadeOut = smoothstep(17.68, 18, seconds);
  const visibleTail = 1 - fadeOut;
  const curtainOpen = smoothstep(1.45, 2.95, seconds);
  const flash = Math.min(1, pulse(seconds, 0.72, 0.11) + pulse(seconds, 1.38, 0.13));

  film.update(frame.elapsedMilliseconds, colourMix);
  setVisualOpacity(cinema, (1 - homeMix) * visibleTail);
  setVisualOpacity(home, homeMix * (1 - phoneMix * 0.78) * visibleTail);
  setVisualOpacity(television, homeMix * (1 - tabletMix) ** 2 * visibleTail);
  setVisualOpacity(tablet, tabletMix * (1 - phoneMix) ** 2 * visibleTail);
  setVisualOpacity(phone, phoneMix * visibleTail);

  cinema.curtainLeft.position.x = -lerp(2.65, 7.85, curtainOpen);
  cinema.curtainRight.position.x = lerp(2.65, 7.85, curtainOpen);
  cinema.screen.scale.x = lerp(0.74, 1, colourMix);
  cinema.screenMaterial.opacity = (1 - homeMix) * Math.min(1, filmStarted + flash * 0.9);
  cinema.screenMaterial.color.setScalar(lerp(0.18, 1, filmStarted) + flash * 0.65);
  cinema.dustMaterial.opacity = (1 - homeMix) * filmStarted * lerp(0.16, 0.27, colourMix);
  cinema.aisleMaterial.opacity = (1 - homeMix) * lerp(0.06, 0.42, wake);
  cinema.dust.rotation.z = seconds * 0.012;
  cinema.dust.position.y = Math.sin(seconds * 0.25) * 0.08;
  cinema.seatMaterial.color.lerpColors(MONO_SEAT, COLOUR_SEAT, colourMix);
  cinema.wallMaterial.color.lerpColors(MONO_WALL, COLOUR_WALL, colourMix);
  cinema.curtainMaterial.color.lerpColors(MONO_CURTAIN, COLOUR_CURTAIN, colourMix);
  cinema.screenLight.intensity = (1 - homeMix) * visibleTail * (filmStarted * lerp(2.2, 5.2, colourMix) + flash * 10);
  cinema.screenLight.color.setRGB(lerp(0.95, 0.34, colourMix), lerp(0.92, 0.78, colourMix), lerp(0.82, 0.9, colourMix));

  home.lampLight.intensity = homeMix * (1 - phoneMix * 0.7) * visibleTail * 30;
  home.accentLight.intensity = homeMix * (1 - phoneMix * 0.5) * visibleTail * 22;
  phone.brandLight.intensity = phoneMix * brandMix * visibleTail * (8 + finalGlow * 18);
  ambient.intensity = visibleTail * lerp(0.05 + flash * 0.11, 0.58, Math.max(colourMix * 0.55, homeMix)) + finalGlow * 0.8;

  television.root.position.set(0, lerp(3.2, 1.45, homeMix), lerp(-9.3, -2.35, homeMix));
  television.root.scale.setScalar(lerp(1.42, 1, homeMix) * lerp(1, 0.86, tabletMix));
  tablet.root.position.set(0, lerp(1.42, 1.55, tabletMix), lerp(-2.1, -1.35, tabletMix));
  tablet.root.scale.setScalar(lerp(1.16, 1, tabletMix) * lerp(1, 0.9, phoneMix));
  phone.root.position.set(0, 1.38, lerp(-1.28, -0.55, phoneMix));
  phone.root.scale.setScalar(lerp(1.1, 1 + brandMix * 0.06, phoneMix));
  phone.filmMaterial.opacity = phoneMix * (1 - brandMix) * visibleTail;
  phone.filmMaterial.visible = phone.filmMaterial.opacity > 0.002;
  phone.brandMaterial.opacity = phoneMix * brandMix * visibleTail;
  phone.brandMaterial.visible = phone.brandMaterial.opacity > 0.002;

  const cinemaTravel = smoothstep(0, 11, seconds);
  const cameraZ = lerp(11.6, 7.1, cinemaTravel);
  const portraitBoost = camera.aspect < 0.72
    ? (0.72 / camera.aspect - 1) * 5.5 * Math.max(colourMix, homeMix) * (1 - phoneMix)
    : 0;
  camera.position.set(
    Math.sin(seconds * 0.24) * 0.07 * (1 - homeMix),
    lerp(2.05, 1.5, homeMix),
    lerp(lerp(cameraZ, 6.15, homeMix), 4.85, phoneMix) + portraitBoost,
  );
  camera.rotation.z = Math.sin(seconds * 0.18) * 0.0025 * (1 - homeMix);
  camera.lookAt(0, lerp(2.45, 1.38, homeMix), lerp(-9.85, -0.82, homeMix));

  const backgroundMix = Math.max(colourMix * 0.22, homeMix);
  (scene.background as THREE.Color)
    .lerpColors(CINEMA_BACKGROUND, HOME_BACKGROUND, backgroundMix)
    .lerp(FINAL_GLOW, finalGlow * 0.3)
    .lerp(CINEMA_BACKGROUND, fadeOut);
  if (scene.fog) scene.fog.color.copy(scene.background as THREE.Color);
}

function applyReducedMotionFrame(
  progress: number,
  { ambient, camera, cinema, film, home, phone, scene, tablet, television }: CinematicWorld,
) {
  const dissolve = smoothstep(0.2, 0.85, progress);
  film.update(9_000, 1);
  setVisualOpacity(cinema, 1 - dissolve);
  setVisualOpacity(home, 0);
  setVisualOpacity(television, 0);
  setVisualOpacity(tablet, 0);
  setVisualOpacity(phone, dissolve);
  cinema.curtainLeft.position.x = -7.85;
  cinema.curtainRight.position.x = 7.85;
  cinema.screen.scale.x = 1;
  cinema.screenMaterial.opacity = 1 - dissolve;
  cinema.dustMaterial.opacity = 0;
  cinema.aisleMaterial.opacity = (1 - dissolve) * 0.25;
  cinema.screenLight.intensity = (1 - dissolve) * 3.5;
  cinema.seatMaterial.color.copy(COLOUR_SEAT);
  cinema.wallMaterial.color.copy(COLOUR_WALL);
  cinema.curtainMaterial.color.copy(COLOUR_CURTAIN);
  phone.root.position.set(0, 1.38, -0.55);
  phone.root.scale.setScalar(1.04);
  phone.filmMaterial.opacity = 0;
  phone.filmMaterial.visible = false;
  phone.brandMaterial.opacity = dissolve;
  phone.brandMaterial.visible = dissolve > 0.002;
  phone.brandLight.intensity = dissolve * 8;
  ambient.intensity = 0.42;
  camera.position.set(0, 1.55, 4.85);
  camera.lookAt(0, 1.38, -0.72);
  (scene.background as THREE.Color).lerpColors(CINEMA_BACKGROUND, HOME_BACKGROUND, dissolve * 0.65);
  if (scene.fog) scene.fog.color.copy(scene.background as THREE.Color);
}

function createCinemaSet(
  track: <T extends DisposableResource>(resource: T) => T,
  filmTexture: THREE.Texture,
  reducedDetail: boolean,
): CinemaSet {
  const root = new THREE.Group();
  const wallMaterial = track(new THREE.MeshStandardMaterial({ color: MONO_WALL, metalness: 0.05, roughness: 0.92, transparent: true }));
  const floorMaterial = track(new THREE.MeshStandardMaterial({ color: 0x14110f, metalness: 0.08, roughness: 0.86, transparent: true }));
  const frameMaterial = track(new THREE.MeshStandardMaterial({ color: 0x090a0a, metalness: 0.55, roughness: 0.4, transparent: true }));
  const seatMaterial = track(new THREE.MeshStandardMaterial({ color: MONO_SEAT, metalness: 0.04, roughness: 0.78, transparent: true }));
  const curtainMaterial = track(new THREE.MeshStandardMaterial({ color: MONO_CURTAIN, metalness: 0.02, roughness: 0.8, transparent: true }));
  const curtainShadeMaterial = track(new THREE.MeshStandardMaterial({ color: 0x3c101b, metalness: 0.02, roughness: 0.9, transparent: true }));
  const screenMaterial = track(new THREE.MeshBasicMaterial({ color: 0xffffff, map: filmTexture, opacity: 0, toneMapped: false, transparent: true }));
  const aisleMaterial = track(new THREE.MeshBasicMaterial({ color: 0xffc97b, opacity: 0, toneMapped: false, transparent: true }));

  const backWall = new THREE.Mesh(track(new THREE.PlaneGeometry(22, 12)), wallMaterial);
  backWall.position.set(0, 3.2, -10.2);
  root.add(backWall);
  const floor = new THREE.Mesh(track(new THREE.PlaneGeometry(28, 30)), floorMaterial);
  floor.rotation.x = -Math.PI / 2;
  floor.position.set(0, -1.25, 1.5);
  root.add(floor);
  const ceiling = new THREE.Mesh(track(new THREE.PlaneGeometry(28, 30)), wallMaterial);
  ceiling.rotation.x = Math.PI / 2;
  ceiling.position.set(0, 7.2, 1.5);
  root.add(ceiling);
  for (const side of [-1, 1]) {
    const wall = new THREE.Mesh(track(new THREE.PlaneGeometry(30, 9)), wallMaterial);
    wall.rotation.y = side * -Math.PI / 2;
    wall.position.set(side * 8.5, 2.9, 1.5);
    root.add(wall);
  }

  const screen = new THREE.Mesh(track(new THREE.PlaneGeometry(10.6, 5.96)), screenMaterial);
  screen.position.set(0, 3.15, -9.94);
  root.add(screen);
  root.add(...createScreenFrame(track, frameMaterial));

  const curtainLeft = createCurtain(track, curtainMaterial, curtainShadeMaterial);
  const curtainRight = createCurtain(track, curtainMaterial, curtainShadeMaterial);
  curtainLeft.position.set(-2.65, 3.15, -9.64);
  curtainRight.position.set(2.65, 3.15, -9.64);
  root.add(curtainLeft, curtainRight);

  const seatBackGeometry = track(new THREE.BoxGeometry(0.82, 0.92, 0.2));
  const seatBaseGeometry = track(new THREE.BoxGeometry(0.82, 0.2, 0.68));
  const columns = reducedDetail ? [-4, -3, -2, 2, 3, 4] : [-5, -4, -3, -2, 2, 3, 4, 5];
  const rows = reducedDetail ? 4 : 6;
  const count = columns.length * rows;
  const seatBacks = new THREE.InstancedMesh(seatBackGeometry, seatMaterial, count);
  const seatBases = new THREE.InstancedMesh(seatBaseGeometry, seatMaterial, count);
  const dummy = new THREE.Object3D();
  let instance = 0;
  for (let row = 0; row < rows; row += 1) {
    for (const column of columns) {
      const x = column * 1.02 + (row % 2 === 0 ? 0.04 : -0.04);
      const z = -1.3 + row * 1.67;
      const y = -0.48 + row * 0.07;
      dummy.position.set(x, y + 0.45, z);
      dummy.rotation.set(-0.06, 0, 0);
      dummy.updateMatrix();
      seatBacks.setMatrixAt(instance, dummy.matrix);
      dummy.position.set(x, y - 0.05, z - 0.28);
      dummy.rotation.set(0, 0, 0);
      dummy.updateMatrix();
      seatBases.setMatrixAt(instance, dummy.matrix);
      instance += 1;
    }
  }
  root.add(seatBacks, seatBases);

  const aisleGeometry = track(new THREE.BoxGeometry(0.07, 0.035, 0.34));
  const aisleLights = new THREE.InstancedMesh(aisleGeometry, aisleMaterial, 14);
  instance = 0;
  for (let row = 0; row < 7; row += 1) {
    for (const side of [-1, 1]) {
      dummy.position.set(side * 1.12, -1.17, -1.4 + row * 1.67);
      dummy.rotation.set(0, 0, 0);
      dummy.updateMatrix();
      aisleLights.setMatrixAt(instance, dummy.matrix);
      instance += 1;
    }
  }
  root.add(aisleLights);

  const dustCount = reducedDetail ? 90 : 180;
  const dustVertices: number[] = [];
  const random = seededRandom(4_091);
  for (let index = 0; index < dustCount; index += 1) {
    const z = lerp(-8.8, 7.8, random());
    const spread = lerp(4.6, 0.18, (z + 8.8) / 16.6);
    dustVertices.push((random() - 0.5) * spread * 2, 2.4 + (random() - 0.5) * spread * 0.65, z);
  }
  const dustGeometry = track(new THREE.BufferGeometry());
  dustGeometry.setAttribute('position', new THREE.Float32BufferAttribute(dustVertices, 3));
  const dustMaterial = track(new THREE.PointsMaterial({
    blending: THREE.AdditiveBlending,
    color: 0xfff4d5,
    depthWrite: false,
    opacity: 0,
    size: reducedDetail ? 0.026 : 0.02,
    sizeAttenuation: true,
    transparent: true,
  }));
  const dust = new THREE.Points(dustGeometry, dustMaterial);
  root.add(dust);

  const screenLight = new THREE.PointLight(0xffefd0, 0, 23, 1.55);
  screenLight.position.set(0, 3.1, -7.2);
  root.add(screenLight);
  const projectorLight = new THREE.PointLight(0xffdca4, 0.5, 14, 2);
  projectorLight.position.set(0, 2.5, 7.5);
  root.add(projectorLight);
  const projectorSpot = new THREE.SpotLight(0xffe4b3, 12, 28, Math.PI / 7, 0.65, 1.4);
  projectorSpot.position.set(0, 2.65, 7.6);
  projectorSpot.target.position.set(0, 3.1, -9.8);
  root.add(projectorSpot, projectorSpot.target);

  return {
    aisleMaterial,
    curtainLeft,
    curtainMaterial,
    curtainRight,
    dust,
    dustMaterial,
    materials: collectMaterials(root),
    root,
    screen,
    screenLight,
    screenMaterial,
    seatMaterial,
    wallMaterial,
  };
}

function createHomeSet(track: <T extends DisposableResource>(resource: T) => T): HomeSet {
  const root = new THREE.Group();
  const wallMaterial = track(new THREE.MeshStandardMaterial({ color: 0x554e44, roughness: 0.94, transparent: true }));
  const floorMaterial = track(new THREE.MeshStandardMaterial({ color: 0x32231e, roughness: 0.76, transparent: true }));
  const tealMaterial = track(new THREE.MeshStandardMaterial({ color: 0x1e5a5b, metalness: 0.1, roughness: 0.58, transparent: true }));
  const woodMaterial = track(new THREE.MeshStandardMaterial({ color: 0x6c4932, roughness: 0.72, transparent: true }));
  const textileMaterial = track(new THREE.MeshStandardMaterial({ color: 0x793827, roughness: 0.95, transparent: true }));
  const brassMaterial = track(new THREE.MeshStandardMaterial({ color: 0xb98d44, metalness: 0.52, roughness: 0.42, transparent: true }));
  const bookMaterials = [
    track(new THREE.MeshStandardMaterial({ color: 0x406d78, roughness: 0.75, transparent: true })),
    track(new THREE.MeshStandardMaterial({ color: 0xa64f3d, roughness: 0.75, transparent: true })),
    track(new THREE.MeshStandardMaterial({ color: 0xc49a52, roughness: 0.75, transparent: true })),
  ];

  const wall = new THREE.Mesh(track(new THREE.PlaneGeometry(18, 10)), wallMaterial);
  wall.position.set(0, 2.6, -3.15);
  root.add(wall);
  const floor = new THREE.Mesh(track(new THREE.PlaneGeometry(20, 14)), floorMaterial);
  floor.rotation.x = -Math.PI / 2;
  floor.position.set(0, -1.28, 1.5);
  root.add(floor);
  const rug = new THREE.Mesh(track(new THREE.PlaneGeometry(8, 5)), textileMaterial);
  rug.rotation.x = -Math.PI / 2;
  rug.position.set(0, -1.25, 0.8);
  root.add(rug);
  root.add(box(track, 7, 0.62, 0.72, woodMaterial, 0, -0.82, -2.45));
  root.add(box(track, 2.3, 0.12, 0.82, tealMaterial, 0, -0.45, -2.25));

  const lampStand = new THREE.Mesh(track(new THREE.CylinderGeometry(0.055, 0.08, 3.6, 14)), brassMaterial);
  lampStand.position.set(4.05, 0.52, -1.65);
  root.add(lampStand);
  const lampShade = new THREE.Mesh(track(new THREE.ConeGeometry(0.78, 1.15, 28, 1, true)), textileMaterial);
  lampShade.position.set(4.05, 2.35, -1.65);
  root.add(lampShade);
  const lampLight = new THREE.PointLight(0xffbd72, 0, 8, 2);
  lampLight.position.set(4.05, 2.05, -1.35);
  root.add(lampLight);

  root.add(box(track, 2.2, 4.3, 0.4, tealMaterial, -4.3, 0.85, -2.68));
  for (let index = 0; index < 8; index += 1) {
    const width = 0.16 + (index % 3) * 0.045;
    const height = 0.66 + (index % 4) * 0.13;
    root.add(box(track, width, height, 0.26, bookMaterials[index % bookMaterials.length] as THREE.Material, -5 + index * 0.21, -0.55 + height / 2, -2.42));
  }
  const sofa = box(track, 7.2, 1.12, 1.25, textileMaterial, 0, -0.65, 3.5);
  sofa.rotation.x = -0.05;
  root.add(sofa);
  root.add(box(track, 0.34, 0.7, 1.4, textileMaterial, -3.72, -0.65, 3.42));
  root.add(box(track, 0.34, 0.7, 1.4, textileMaterial, 3.72, -0.65, 3.42));

  const accentLight = new THREE.PointLight(0x4dc4c5, 0, 9, 2);
  accentLight.position.set(-3.4, 2.1, -1.4);
  root.add(accentLight);
  return { accentLight, lampLight, materials: collectMaterials(root), root };
}

function createTelevision(track: <T extends DisposableResource>(resource: T) => T, filmTexture: THREE.Texture): DeviceSet {
  const device = createDevice(track, filmTexture, { contentHeight: 3.38, contentWidth: 6, depth: 0.24, height: 3.72, radius: 0.16, width: 6.34 });
  const standMaterial = track(new THREE.MeshStandardMaterial({ color: 0x111416, metalness: 0.62, roughness: 0.33, transparent: true }));
  device.root.add(box(track, 0.2, 0.72, 0.22, standMaterial, 0, -2.15, -0.02));
  device.root.add(box(track, 1.75, 0.11, 0.7, standMaterial, 0, -2.5, 0));
  return { ...device, materials: collectMaterials(device.root) };
}

function createTablet(track: <T extends DisposableResource>(resource: T) => T, filmTexture: THREE.Texture): DeviceSet {
  const device = createDevice(track, filmTexture, { contentHeight: 2.28, contentWidth: 4, depth: 0.18, height: 2.82, radius: 0.24, width: 4.34 });
  const cameraMaterial = track(new THREE.MeshBasicMaterial({ color: 0x78858a, toneMapped: false, transparent: true }));
  const camera = new THREE.Mesh(track(new THREE.CircleGeometry(0.045, 18)), cameraMaterial);
  camera.position.set(0, 1.27, 0.12);
  device.root.add(camera);
  return { ...device, materials: collectMaterials(device.root) };
}

function createPhone(
  track: <T extends DisposableResource>(resource: T) => T,
  filmTexture: THREE.Texture,
  brandTexture: THREE.Texture,
): PhoneSet {
  const root = new THREE.Group();
  const caseMaterial = track(new THREE.MeshStandardMaterial({ color: 0x0b1012, metalness: 0.72, roughness: 0.28, transparent: true }));
  const screenBackMaterial = track(new THREE.MeshBasicMaterial({ color: 0x020303, toneMapped: false, transparent: true }));
  const filmMaterial = track(new THREE.MeshBasicMaterial({ color: 0xffffff, map: filmTexture, toneMapped: false, transparent: true }));
  const brandMaterial = track(new THREE.MeshBasicMaterial({ color: 0xffffff, map: brandTexture, opacity: 1, toneMapped: false, transparent: true }));
  const caseMesh = new THREE.Mesh(track(roundedPanelGeometry(2.12, 4.34, 0.34, 0.2)), caseMaterial);
  root.add(caseMesh);
  const screenBack = new THREE.Mesh(track(new THREE.PlaneGeometry(1.9, 4.02)), screenBackMaterial);
  screenBack.position.z = 0.16;
  root.add(screenBack);
  const filmScreen = new THREE.Mesh(track(new THREE.PlaneGeometry(1.9, 1.07)), filmMaterial);
  filmScreen.position.set(0, 0.18, 0.17);
  root.add(filmScreen);
  const brandScreen = new THREE.Mesh(track(new THREE.PlaneGeometry(1.9, 4.02)), brandMaterial);
  brandScreen.position.z = 0.18;
  root.add(brandScreen);
  const brandLight = new THREE.PointLight(0xffe7b8, 0, 7, 2);
  brandLight.position.set(0, 0.2, 1.4);
  root.add(brandLight);
  const notch = box(track, 0.52, 0.075, 0.035, caseMaterial, 0, 1.86, 0.2);
  root.add(notch);
  return { brandLight, brandMaterial, filmMaterial, materials: collectMaterials(root), root };
}

function createDevice(
  track: <T extends DisposableResource>(resource: T) => T,
  filmTexture: THREE.Texture,
  dimensions: { contentHeight: number; contentWidth: number; depth: number; height: number; radius: number; width: number },
): DeviceSet {
  const root = new THREE.Group();
  const frameMaterial = track(new THREE.MeshStandardMaterial({ color: 0x0d1113, metalness: 0.66, roughness: 0.3, transparent: true }));
  const filmMaterial = track(new THREE.MeshBasicMaterial({ color: 0xffffff, map: filmTexture, toneMapped: false, transparent: true }));
  const frame = new THREE.Mesh(track(roundedPanelGeometry(dimensions.width, dimensions.height, dimensions.radius, dimensions.depth)), frameMaterial);
  root.add(frame);
  const screen = new THREE.Mesh(track(new THREE.PlaneGeometry(dimensions.contentWidth, dimensions.contentHeight)), filmMaterial);
  screen.position.z = dimensions.depth / 2 + 0.07;
  root.add(screen);
  return { filmMaterial, materials: collectMaterials(root), root };
}

function createScreenFrame(track: <T extends DisposableResource>(resource: T) => T, material: THREE.Material): THREE.Mesh[] {
  return [
    box(track, 11.1, 0.2, 0.28, material, 0, 6.23, -9.84),
    box(track, 11.1, 0.2, 0.28, material, 0, 0.07, -9.84),
    box(track, 0.2, 6.35, 0.28, material, -5.55, 3.15, -9.84),
    box(track, 0.2, 6.35, 0.28, material, 5.55, 3.15, -9.84),
  ];
}

function createCurtain(
  track: <T extends DisposableResource>(resource: T) => T,
  material: THREE.Material,
  shadeMaterial: THREE.Material,
): THREE.Group {
  const root = new THREE.Group();
  for (let index = 0; index < 7; index += 1) {
    const strip = box(track, 0.86, 6.95, 0.18, index % 2 === 0 ? material : shadeMaterial, (index - 3) * 0.77, 0, 0);
    strip.rotation.y = (index - 3) * 0.012;
    root.add(strip);
  }
  return root;
}

function createBrandTexture(): { dispose: () => void; texture: THREE.CanvasTexture } {
  const canvas = document.createElement('canvas');
  canvas.width = 512;
  canvas.height = 1_024;
  const context = canvas.getContext('2d', { alpha: false });
  if (!context) throw new Error('Canvas 2D is unavailable.');
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.magFilter = THREE.LinearFilter;
  texture.minFilter = THREE.LinearFilter;
  texture.generateMipmaps = false;

  const draw = (image?: HTMLImageElement) => {
    context.fillStyle = '#050708';
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.fillStyle = '#f1eed7';
    roundedRect(context, 136, 292, 240, 240, 34);
    context.fill();
    if (image) context.drawImage(image, 152, 308, 208, 208);
    else drawProjectorMark(context);
    context.fillStyle = '#f7f8f5';
    context.font = '700 58px system-ui, sans-serif';
    context.textAlign = 'center';
    context.textBaseline = 'middle';
    context.fillText('TJXY', 256, 610);
    context.fillStyle = '#58b8b1';
    context.fillRect(188, 662, 136, 4);
    texture.needsUpdate = true;
  };
  draw();
  const image = new Image();
  image.decoding = 'async';
  image.onload = () => { draw(image); };
  image.onerror = () => undefined;
  image.src = '/brand/tjxy-mark.webp';

  return {
    dispose: () => {
      image.onload = null;
      image.onerror = null;
      texture.dispose();
    },
    texture,
  };
}

function drawProjectorMark(context: CanvasRenderingContext2D) {
  context.strokeStyle = '#252923';
  context.fillStyle = '#252923';
  context.lineWidth = 8;
  context.beginPath();
  context.arc(256, 392, 58, 0, Math.PI * 2);
  context.stroke();
  for (const angle of [0, Math.PI * 2 / 3, Math.PI * 4 / 3]) {
    context.beginPath();
    context.arc(256 + Math.cos(angle) * 30, 392 + Math.sin(angle) * 30, 10, 0, Math.PI * 2);
    context.fill();
  }
  context.strokeRect(210, 458, 92, 40);
  context.beginPath();
  context.moveTo(302, 465);
  context.lineTo(338, 448);
  context.lineTo(338, 492);
  context.lineTo(302, 480);
  context.closePath();
  context.stroke();
}

function roundedPanelGeometry(width: number, height: number, radius: number, depth: number): THREE.ExtrudeGeometry {
  const shape = new THREE.Shape();
  const left = -width / 2;
  const right = width / 2;
  const bottom = -height / 2;
  const top = height / 2;
  shape.moveTo(left + radius, bottom);
  shape.lineTo(right - radius, bottom);
  shape.quadraticCurveTo(right, bottom, right, bottom + radius);
  shape.lineTo(right, top - radius);
  shape.quadraticCurveTo(right, top, right - radius, top);
  shape.lineTo(left + radius, top);
  shape.quadraticCurveTo(left, top, left, top - radius);
  shape.lineTo(left, bottom + radius);
  shape.quadraticCurveTo(left, bottom, left + radius, bottom);
  const geometry = new THREE.ExtrudeGeometry(shape, {
    bevelEnabled: true,
    bevelSegments: 3,
    bevelSize: 0.035,
    bevelThickness: 0.025,
    curveSegments: 8,
    depth,
  });
  geometry.center();
  return geometry;
}

function box(
  track: <T extends DisposableResource>(resource: T) => T,
  width: number,
  height: number,
  depth: number,
  material: THREE.Material,
  x: number,
  y: number,
  z: number,
): THREE.Mesh {
  const mesh = new THREE.Mesh(track(new THREE.BoxGeometry(width, height, depth)), material);
  mesh.position.set(x, y, z);
  return mesh;
}

function collectMaterials(root: THREE.Object3D): TrackedMaterial[] {
  const materials = new Map<THREE.Material, number>();
  root.traverse((object) => {
    const renderable = object as THREE.Object3D & { material?: THREE.Material | THREE.Material[] };
    const candidate = renderable.material;
    if (!candidate) return;
    for (const material of Array.isArray(candidate) ? candidate : [candidate]) {
      if (!materials.has(material)) materials.set(material, material.opacity);
      material.transparent = true;
    }
  });
  return [...materials].map(([material, opacity]) => ({ material, opacity }));
}

function setVisualOpacity(visual: Visual, opacity: number) {
  const clamped = clamp01(opacity);
  visual.root.visible = clamped > 0.002;
  for (const tracked of visual.materials) {
    tracked.material.opacity = tracked.opacity * clamped;
    tracked.material.visible = clamped > 0.002;
  }
}

function roundedRect(
  context: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
) {
  context.beginPath();
  context.moveTo(x + radius, y);
  context.lineTo(x + width - radius, y);
  context.quadraticCurveTo(x + width, y, x + width, y + radius);
  context.lineTo(x + width, y + height - radius);
  context.quadraticCurveTo(x + width, y + height, x + width - radius, y + height);
  context.lineTo(x + radius, y + height);
  context.quadraticCurveTo(x, y + height, x, y + height - radius);
  context.lineTo(x, y + radius);
  context.quadraticCurveTo(x, y, x + radius, y);
  context.closePath();
}

function seededRandom(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state += 0x6d2b79f5;
    let value = state;
    value = Math.imul(value ^ value >>> 15, value | 1);
    value ^= value + Math.imul(value ^ value >>> 7, value | 61);
    return ((value ^ value >>> 14) >>> 0) / 4_294_967_296;
  };
}

function smoothstep(minimum: number, maximum: number, value: number): number {
  const amount = clamp01((value - minimum) / (maximum - minimum));
  return amount * amount * (3 - 2 * amount);
}

function pulse(value: number, centre: number, width: number): number {
  const distance = (value - centre) / width;
  return Math.exp(-distance * distance);
}

function lerp(from: number, to: number, amount: number): number {
  return from + (to - from) * amount;
}

function clamp01(value: number): number {
  return Math.min(Math.max(value, 0), 1);
}
