import * as THREE from 'three';
import { RoundedBoxGeometry } from 'three/addons/geometries/RoundedBoxGeometry.js';

export interface Model {
  root: THREE.Group;
  reels?: THREE.Group[];
}

const metal = (color: number, roughness = 0.3, metalness = 0.8) =>
  new THREE.MeshStandardMaterial({ color, roughness, metalness });

function box(parent: THREE.Object3D, size: [number, number, number], at: [number, number, number], material: THREE.Material, radius = 0.06) {
  const mesh = new THREE.Mesh(new RoundedBoxGeometry(...size, 3, Math.min(radius, Math.min(...size) / 2)), material);
  mesh.position.set(...at);
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  parent.add(mesh);
  return mesh;
}

function cylinder(parent: THREE.Object3D, radius: number, depth: number, at: [number, number, number], material: THREE.Material) {
  const mesh = new THREE.Mesh(new THREE.CylinderGeometry(radius, radius, depth, 48), material);
  mesh.rotation.x = Math.PI / 2;
  mesh.position.set(...at);
  mesh.castShadow = true;
  parent.add(mesh);
  return mesh;
}

function rod(parent: THREE.Object3D, from: THREE.Vector3, to: THREE.Vector3, radius: number, material: THREE.Material) {
  const direction = to.clone().sub(from);
  const mesh = new THREE.Mesh(new THREE.CylinderGeometry(radius, radius, direction.length(), 12), material);
  mesh.position.copy(from).add(to).multiplyScalar(0.5);
  mesh.quaternion.setFromUnitVectors(new THREE.Vector3(0, 1, 0), direction.normalize());
  parent.add(mesh);
}

function label(parent: THREE.Object3D, text: string, width: number, at: [number, number, number], color = '#ccc8b8') {
  const canvas = document.createElement('canvas');
  canvas.width = 512;
  canvas.height = 64;
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('Canvas 2D is unavailable');
  ctx.fillStyle = color;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.font = '500 28px monospace';
  ctx.fillText(text, 256, 32);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  const mesh = new THREE.Mesh(new THREE.PlaneGeometry(width, width / 8), new THREE.MeshBasicMaterial({ map: texture, transparent: true, depthWrite: false }));
  mesh.position.set(...at);
  parent.add(mesh);
}

export function createFilmMaterial() {
  return new THREE.ShaderMaterial({
    uniforms: { uTime: { value: 0 }, uColor: { value: 0 }, uCrt: { value: 0 }, uAspect: { value: 1.6 }, uOpacity: { value: 1 } },
    vertexShader: `varying vec2 vUv;
      void main() { vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }`,
    fragmentShader: `
      varying vec2 vUv;
      uniform float uTime, uColor, uCrt, uAspect, uOpacity;
      float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1,311.7))) * 43758.5453); }
      void main() {
        vec2 uv = vUv;
        float time = uTime * .12;
        vec3 sky = mix(vec3(.055,.15,.25), vec3(.90,.52,.29), pow(1.-uv.y, 1.5));
        vec2 sunUV = (uv - vec2(.68,.63)) * vec2(uAspect,1.);
        float sun = 1. - smoothstep(.085,.09,length(sunUV));
        sky = mix(sky, vec3(1.,.83,.52), sun);
        sky += vec3(.25,.10,.035) * exp(-length(sunUV)*8.);
        float ridge = .40 + sin(uv.x*7. + .5)*.09 + sin(uv.x*17.+1.)*.035;
        vec3 col = mix(sky, vec3(.22,.29,.36), 1.-smoothstep(ridge-.005,ridge+.005,uv.y));
        ridge = .29 + sin(uv.x*9.+time)*.055 + sin(uv.x*19.)*.035;
        col = mix(col,vec3(.09,.22,.28),1.-smoothstep(ridge-.004,ridge+.004,uv.y));
        float water = 1.-smoothstep(.235,.24,uv.y);
        vec3 lake = mix(vec3(.045,.11,.18),vec3(.19,.34,.37),uv.y*4.);
        float reflection = exp(-pow((uv.x-.68+sin(uv.y*260.+time*4.)*.026)*6.,2.));
        lake += vec3(.65,.33,.13)*reflection*(.25+.75*pow(sin(uv.y*430.+time*5.)*.5+.5,5.));
        col = mix(col,lake,water);
        float hill = .105+sin(uv.x*8.-1.)*.04;
        col = mix(col,vec3(.018,.045,.06),1.-smoothstep(hill-.003,hill+.003,uv.y));
        float grain = hash(floor(uv*vec2(900.,540.))+floor(uTime*18.))-.5;
        float gray = dot(col,vec3(.299,.587,.114));
        col = mix(vec3(gray)*vec3(1.02,1.,.94),col,uColor);
        col += grain * mix(.07,.012,uColor);
        float vignette = 1. - .7*pow(length((uv-.5)*1.35),2.);
        col *= vignette;
        col *= 1. - uCrt * (.06 + .07*sin(uv.y*650.));
        col *= 1. - (1.-uColor)*.025*sin(uTime*45.);
        gl_FragColor = vec4(col,uOpacity);
        #include <tonemapping_fragment>
        #include <colorspace_fragment>
      }`,
  });
}

export function screen(parent: THREE.Object3D, width: number, height: number, at: [number, number, number], material: THREE.Material, curved = false) {
  const geometry = new THREE.PlaneGeometry(width, height, curved ? 48 : 1, curved ? 32 : 1);
  if (curved) {
    const positions = geometry.attributes.position;
    if (positions) for (let i = 0; i < positions.count; i++) {
      const x = positions.getX(i) / (width / 2);
      const y = positions.getY(i) / (height / 2);
      positions.setZ(i, .19 * (1 - x*x) * (1 - y*y));
    }
    geometry.computeVertexNormals();
  }
  const mesh = new THREE.Mesh(geometry, material);
  mesh.position.set(...at);
  parent.add(mesh);
  return mesh;
}

export function createProjector(): Model {
  const root = new THREE.Group();
  const housing = metal(0x344247, .4, .65);
  const dark = metal(0x121a1e, .32);
  const silver = metal(0xb7b7a9, .24);
  const brass = metal(0xb18a4e, .26);
  const rubber = metal(0x151515, .85, 0);
  box(root, [1.65,1.22,.83], [0,.03,0], housing, .14);
  box(root, [1.42,1,.045], [0,.02,.44], dark, .1);
  box(root, [.58,.12,.12], [0,.75,0], brass);
  for (const x of [-.6,.6]) {
    box(root, [.14,.3,.15], [x,-.72,0], silver);
    box(root, [.38,.14,.75], [x,-.91,0], rubber);
  }
  box(root, [2,.12,1], [0,-1,0], dark);
  for (let i=0; i<9; i++) box(root,[.025,.38,.015],[-.57+i*.085,-.04,.472],housing,.005);
  for (const x of [-.68,.68]) for (const y of [-.39,.4]) cylinder(root,.032,.024,[x,y,.477],silver);
  cylinder(root,.17,.08,[.49,.04,.49],brass);
  cylinder(root,.11,.1,[.49,.04,.54],dark);
  label(root,'TJXY · 35 MM',.83,[0,-.35,.479]);
  const lens = new THREE.Group();
  lens.rotation.y = Math.PI/2;
  lens.position.set(.9,.16,0);
  cylinder(lens,.26,.42,[0,0,0],dark);
  cylinder(lens,.3,.08,[0,0,.23],brass);
  cylinder(lens,.24,.07,[0,0,.29],silver);
  cylinder(lens,.20,.025,[0,0,.335],new THREE.MeshStandardMaterial({ color:0xffedbf,emissive:0xffbb55,emissiveIntensity:3,roughness:.1,metalness:.2 }));
  for (let i=0;i<5;i++) cylinder(lens,.27,.017,[0,0,-.12+i*.05],silver);
  root.add(lens);
  const reels: THREE.Group[] = [];
  for (const [x,y] of [[-.67,1.17],[.64,1.42]]) {
    if (x === undefined || y === undefined) continue;
    rod(root,new THREE.Vector3(x*.4,.5,-.13),new THREE.Vector3(x,y,-.13),.085,housing);
    const reel = new THREE.Group();
    reel.position.set(x,y,.08);
    cylinder(reel,.65,.16,[0,0,0],dark);
    for (const z of [-.12,.12]) {
      const shape = new THREE.Shape();
      shape.absarc(0,0,.7,0,Math.PI*2,false);
      for (let i=0;i<5;i++) {
        const angle = i*Math.PI*2/5;
        const hole = new THREE.Path();
        hole.absarc(Math.cos(angle)*.4,Math.sin(angle)*.4,.18,0,Math.PI*2,true);
        shape.holes.push(hole);
      }
      const mesh = new THREE.Mesh(new THREE.ExtrudeGeometry(shape,{depth:.035,bevelEnabled:true,bevelSegments:2,steps:1,bevelSize:.009,bevelThickness:.009,curveSegments:24}),silver);
      mesh.position.z=z;
      reel.add(mesh);
      cylinder(reel,.11,.06,[0,0,z+.035],brass);
      const rim = new THREE.Mesh(new THREE.TorusGeometry(.677,.016,8,64),brass);
      rim.position.z=z+.04;
      reel.add(rim);
    }
    reels.push(reel);
    root.add(reel);
  }
  // The dark film runs from each reel into the mechanism.
  rod(root,new THREE.Vector3(-.27,1.02,0),new THREE.Vector3(-.47,.35,0),.025,rubber);
  rod(root,new THREE.Vector3(.25,1.1,0),new THREE.Vector3(.35,.38,0),.025,rubber);
  return {root,reels};
}

export function createCinema(film: THREE.ShaderMaterial): Model {
  const root = new THREE.Group();
  const frame = metal(0x333533,.4);
  box(root,[5.75,3.4,.15],[0,.1,0],frame,.08);
  screen(root,5.5,3.1,[0,.1,.09],film);
  for (const x of [-2.62,2.62]) {
    box(root,[.08,.65,.08],[x,-1.86,0],frame);
    box(root,[.65,.08,.8],[x,-2.17,0],frame);
  }
  return {root};
}

export function createCRT(film: THREE.ShaderMaterial): Model {
  const root = new THREE.Group();
  const woodCanvas = document.createElement('canvas');
  woodCanvas.width=256; woodCanvas.height=256;
  const ctx = woodCanvas.getContext('2d');
  if (!ctx) throw new Error('Canvas 2D is unavailable');
  ctx.fillStyle='#68402c'; ctx.fillRect(0,0,256,256);
  for(let i=0;i<180;i++) {
    ctx.strokeStyle=`rgba(25,10,4,${String(.08+(Math.sin(i*9.3)*.5+.5)*.13)})`;
    ctx.beginPath(); ctx.moveTo(0,i*1.5);
    ctx.bezierCurveTo(90,i*1.5+Math.sin(i)*4,170,i*1.5-4,256,i*1.5+2); ctx.stroke();
  }
  const woodTexture = new THREE.CanvasTexture(woodCanvas);
  woodTexture.colorSpace=THREE.SRGBColorSpace;
  const wood = new THREE.MeshStandardMaterial({map:woodTexture,color:0x9c826e,roughness:.48,metalness:.1});
  const cream = metal(0x9b9680,.43,.4);
  const dark = metal(0x171d1c,.48,.2);
  const silver = metal(0xada891,.3);
  box(root,[4.5,3,1.6],[0,0,-.25],wood,.18);
  box(root,[4.29,2.79,.12],[0,0,.60],cream,.15);
  box(root,[3.48,2.59,.13],[-.31,0,.7],dark,.22);
  box(root,[3.27,2.38,.08],[-.31,0,.79],cream,.2);
  screen(root,3.09,2.22,[-.31,0,.842],film,true);
  // A subtly reflective curved cover gives the tube a glass highlight.
  const glass = new THREE.MeshPhysicalMaterial({color:0x8bbab5,transparent:true,opacity:.07,roughness:.09,metalness:.4,clearcoat:1,depthWrite:false});
  const cover = screen(root,3.09,2.22,[-.31,0,.846],film,true);
  cover.material=glass;
  for (const y of [.76,.17]) {
    cylinder(root,.235,.12,[1.72,y,.8],dark);
    cylinder(root,.183,.13,[1.72,y,.87],silver);
    const tick = box(root,[.027,.16,.016],[1.72,y+.025,.948],dark,.005);
    tick.rotation.z=-.5;
  }
  for(let i=0;i<9;i++) box(root,[.39,.027,.028],[1.72,-.36-i*.071,.79],dark,.004);
  label(root,'TJXY',.41,[1.72,1.18,.78]);
  for (const x of [-1.65,1.65]) {
    box(root,[.22,.32,.67],[x,-1.62,-.2],dark);
    rod(root,new THREE.Vector3(x*.18,1.54,-.25),new THREE.Vector3(x*.72,2.85,-.35),.018,silver);
    cylinder(root,.045,.07,[x*.72,2.85,-.35],silver);
  }
  box(root,[.55,.12,.35],[0,1.54,-.25],dark);
  return {root};
}

export function createLCD(film: THREE.ShaderMaterial): Model {
  const root = new THREE.Group();
  const dark=metal(0x13191e,.22);
  const silver=metal(0x8a9296,.24);
  box(root,[6.25,3.65,.13],[0,0,0],silver,.065);
  box(root,[6.18,3.58,.055],[0,0,.075],dark,.045);
  screen(root,6.04,3.4,[0,.025,.107],film);
  for(const x of [-2.15,2.15]) {
    rod(root,new THREE.Vector3(x,-1.75,0),new THREE.Vector3(x+Math.sign(x)*.35,-2.15,.5),.046,silver);
    rod(root,new THREE.Vector3(x,-1.75,0),new THREE.Vector3(x-Math.sign(x)*.2,-2.15,-.4),.04,dark);
  }
  label(root,'TJXY',.26,[0,-1.755,.11]);
  return {root};
}

export function createLaptop(film: THREE.ShaderMaterial): Model {
  const root = new THREE.Group();
  const silver=metal(0xa4adb3,.25);
  const dark=metal(0x0c1015,.4,.3);
  const lid = new THREE.Group();
  lid.position.set(0,.2,-.44);
  lid.rotation.x=-.12;
  box(lid,[3.88,2.47,.115],[0,.6,0],silver,.1);
  box(lid,[3.72,2.31,.035],[0,.6,.069],dark,.07);
  screen(lid,3.54,2.13,[0,.62,.09],film);
  cylinder(lid,.021,.01,[0,1.72,.092],silver);
  root.add(lid);
  const base=box(root,[3.88,.12,2.4],[0,-.53,.56],silver,.065);
  base.rotation.x=.035;
  box(root,[3.43,.028,1.08],[0,-.453,.18],dark,.055);
  for(let row=0;row<4;row++) for(let col=0;col<13;col++) {
    box(root,[.215,.022,.195],[-1.54+col*.256,-.429,-.19+row*.245],silver,.018);
  }
  box(root,[1.35,.011,.68],[0,-.445,1.13],metal(0x8d989f,.35),.045);
  box(root,[.7,.022,.04],[0,-.54,1.77],dark,.015);
  return {root};
}

export function createHandheld(film: THREE.ShaderMaterial, phone: boolean): Model {
  const root = new THREE.Group();
  const w=phone?1.17:2.48, h=phone?2.42:3.29;
  const rim=metal(phone?0xbdafa0:0x8a9aa4,.2);
  const dark=metal(0x0a1015,.2,.55);
  box(root,[w,h,.12],[0,0,0],rim,phone?.14:.13);
  box(root,[w-.045,h-.045,.04],[0,0,.067],dark,.12);
  // Rounded screen silhouette masks the shader's corners with a shaped geometry.
  const shape = new THREE.Shape();
  const left=-(w-.13)/2,bottom=-(h-.15)/2,width=w-.13,height=h-.15,r=.095;
  shape.moveTo(left+r,bottom); shape.lineTo(left+width-r,bottom);
  shape.quadraticCurveTo(left+width,bottom,left+width,bottom+r);
  shape.lineTo(left+width,bottom+height-r); shape.quadraticCurveTo(left+width,bottom+height,left+width-r,bottom+height);
  shape.lineTo(left+r,bottom+height); shape.quadraticCurveTo(left,bottom+height,left,bottom+height-r);
  shape.lineTo(left,bottom+r); shape.quadraticCurveTo(left,bottom,left+r,bottom);
  const geometry = new THREE.ShapeGeometry(shape,16);
  const pos=geometry.attributes.position, uv=geometry.attributes.uv;
  if(pos&&uv) for(let i=0;i<pos.count;i++) uv.setXY(i,(pos.getX(i)-left)/width,(pos.getY(i)-bottom)/height);
  const display=new THREE.Mesh(geometry,film); display.position.z=.09; root.add(display);
  if(phone) {
    box(root,[.35,.075,.015],[0,h/2-.2,.11],dark,.035);
    box(root,[.32,.022,.008],[0,-h/2+.14,.107],metal(0xdddddd,.3),.01);
  } else cylinder(root,.023,.013,[0,h/2-.043,.091],rim);
  box(root,[.025,.3,.055],[-w/2-.01,.47,0],rim,.01);
  return {root};
}
