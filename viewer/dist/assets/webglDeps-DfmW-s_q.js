import{a as c}from"./VertexArrayObject-BBvFZ8MM.js";import{l as b,d as x,s as y,u as g}from"./VertexArrayObject-BBvFZ8MM.js";import{s as v}from"./ProgramCache-CA3aFXvl.js";import{i0 as h}from"./index-lasFETI3.js";function u(t){const{options:e,value:o}=t;return typeof e[o]=="number"}function $(t){let e="";for(const o in t){const n=t[o];if(typeof n=="boolean")n&&(e+=`#define ${o}
`);else if(typeof n=="number")e+=`#define ${o} ${n.toFixed()}
`;else if(typeof n=="object")if(u(n)){const{value:r,options:f,namespace:s}=n,a=s?`${s}_`:"";for(const i in f)e+=`#define ${a}${i} ${f[i].toFixed()}
`;e+=`#define ${o} ${a}${r}
`}else{const r=n.options;let f=0;for(const s in r)e+=`#define ${r[s]} ${(f++).toFixed()}
`;e+=`#define ${o} ${r[n.value]}
`}}return e}function d(t,e,o,n=""){return new c(t,n+e.vertexShader,n+e.fragmentShader,o)}export{b as BufferObject,c as DisposableProgram,x as FramebufferObject,v as ProgramCache,y as Renderbuffer,h as Texture,g as VertexArrayObject,d as createProgram,$ as glslifyDefineMap};
