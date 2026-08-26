import{o as w,cI as he,ix as Ta,eM as _a,fb as Ca,iT as Ia,eW as se,iI as Da,rn as $a,ko as za,zm as Wt,cq as Bt,ac as c,ae as N,af as j,zn as Pa,zo as Fa,dq as Gt,j3 as Xe,ja as le,iR as bt,iG as ja,nm as yt,lL as K,kp as X,ha as Ke,_ as E,fR as fe,ir as Oa,i0 as Ra,dd as Nt,iq as Wa,qw as Et,dQ as Ba,yy as G,jM as Ga,hv as Na,h$ as We,g7 as Ea,eS as Be,yL as La,eV as wt,p_ as Va,yM as Aa,p$ as Ha,cK as Ua,cy as ka,eX as qa,eU as Mt,cv as Za,fB as Ja,cm as Ya}from"./index-lasFETI3.js";import{t as i,n as f,r as x,w as U,e as b,y as Lt,k as W,z as Ie,i as De,A as Xa,B as Ka,a as m,c as Vt,j as ce,m as Qa,p as $e,C as St}from"./getEmissions.glsl-DLg9S9NB.js";import{t as eo,c as ze,a as Q,i as to,r as Qe,n as At,b as ao}from"./BooleanBindUniform-VWNMtzUi.js";import{J as R,an as B,l as Ht,a3 as we,ao as oo,ap as io,K as ro,aq as et,ar as Ut,as as kt,at as ye,au as no,M as qt,a as so,f as k,u as ne,h as q,O as lo,b as tt,n as ee,t as L,N as O,a4 as Me,av as co,aw as uo,d as Se,g as Zt,B as Jt,p as ho,q as mo,w as po,z as fo,o as vo,r as go,v as xo,A as bo,s as yo,y as wo,V as Mo}from"./OutputColorHighlightOLID.glsl-CT0JXUQY.js";import{r as So,o as Z,d as To,b as _o,s as Co,l as Io,i as Do,a as $o}from"./VertexColor.glsl-D-XCHCPS.js";import{a as zo,n as Po,c as Yt,d as at,o as me,h as Xt,b as J,f as ot,_ as Fo,e as jo,j as Oo}from"./SceneLighting-WYYKqd35.js";import{t as Ae,Q as it}from"./InterleavedLayout-CD2RdENJ.js";import{i as F,L as Kt,Q as Te,J as Tt}from"./BufferView-XM-TXjbv.js";import{r as Ro}from"./VertexBuffer-XU3yhR4e.js";import{g as Wo}from"./mathUtils-CaTLrQDP.js";import{O as te,g as ae,_ as Bo}from"./renderState-BhR1EdRH.js";import{s as V}from"./ShaderBuilder-B0f4Qr9Q.js";import{c as A}from"./NoParameters-DB-WZ6gy.js";function Pe(t,e){switch(t.fragment.code.add(i`vec3 screenDerivativeNormal(vec3 positionView) {
return normalize(cross(dFdx(positionView), dFdy(positionView)));
}`),e.normalType){case 1:t.attributes.add("normalCompressed","vec2"),t.vertex.code.add(i`vec3 decompressNormal(vec2 normal) {
float z = 1.0 - abs(normal.x) - abs(normal.y);
return vec3(normal + sign(normal) * min(z, 0.0), z);
}
vec3 normalModel() {
return decompressNormal(normalCompressed);
}`);break;case 0:t.attributes.add("normal","vec3"),t.vertex.code.add(i`vec3 normalModel() {
return normal;
}`);break;default:e.normalType;case 2:case 3:}}function Go(t){t.uniforms.add(new R("dpDummy",()=>1)).code.add(i`vec3 dpAdd(vec3 hiA, vec3 loA, vec3 hiB, vec3 loB) {
vec3 hiD = hiA + hiB;
vec3 loD = loA + loB;
return  dpDummy * hiD + loD;
}`)}let No=class extends A{constructor(){super(...arguments),this.transformWorldFromViewTH=w(),this.transformWorldFromViewTL=w(),this.transformViewFromCameraRelativeRS=he()}},Eo=class extends A{constructor(){super(...arguments),this.transformWorldFromModelRS=he(),this.transformWorldFromModelTH=w(),this.transformWorldFromModelTL=w(),this.transformationDrawId=0}};function Qt(t,e){const{vertex:a,varyings:o}=t;switch(e.normalType){case 0:case 1:t.include(Pe,e),o.add("vNormalWorld","vec3"),o.add("vNormalView","vec3"),a.uniforms.add(new B("transformNormalViewFromGlobal",r=>r.transformNormalViewFromGlobal)),a.code.add(i`void forwardNormal() {
vNormalWorld = normalModel();
vNormalView = transformNormalViewFromGlobal * vNormalWorld;
}`);break;case 2:t.vertex.code.add(i`void forwardNormal() {}`);break;default:e.normalType;case 3:}}let Lo=class extends No{constructor(){super(...arguments),this.transformNormalViewFromGlobal=he()}},Vo=class extends Eo{constructor(){super(...arguments),this.toMapSpace=Ta()}},Ao=class{constructor(e,a,o){this.elementSize=a.stride,this._buffer=new Ro(e,Ae(a,1)),this.resize(o)}destroy(){this._buffer.dispose()}get capacity(){return this._capacity}get array(){return this._array}get buffer(){return this._buffer}get usedMemory(){return this._array.byteLength+this._buffer.usedMemory}copyRange(e,a,o,r=0){const n=new Uint8Array(this.array,e*this.elementSize,(a-e)*this.elementSize);new Uint8Array(o.array,r*this.elementSize).set(n)}transferAll(){this._buffer.setData(this._array)}transferRange(e,a){const o=e*this.elementSize,r=a*this.elementSize;this._buffer.setSubData(new Uint8Array(this._array),o,o,r)}resize(e){const a=e*this.elementSize,o=new ArrayBuffer(a);this._array&&(e>=this._capacity?new Uint8Array(o).set(new Uint8Array(this._array)):new Uint8Array(o).set(new Uint8Array(this._array).subarray(0,e*this.elementSize))),this._array=o,this._buffer.setSize(a),this._capacity=e}},_t=class{constructor(e){this.localTransform=e.localTransform,this.globalTransform=e.globalTransform,this.modelOrigin=e.modelOrigin,this.model=e.instanceModel,this.modelNormal=e.instanceModelNormal,this.modelScaleFactors=e.modelScaleFactors,this.boundingSphere=e.boundingSphere,this.featureAttribute=e.getField("instanceFeatureAttribute",Kt),this.color=e.getField("instanceColor",Te),this.olidColor=e.getField("instanceOlidColor",Te),this.state=e.getField("state",Tt),this.lodLevel=e.getField("lodLevel",Tt)}},oe=class extends _a{constructor(e,a){super(e),this.events=new Ca,this._capacity=0,this._size=0,this._next=0,this._highlightOptionsMap=new Map,this._highlightOptionsMapPrev=new Map,this._layout=ko(a),this._capacity=_e,this._buffer=this._layout.createBuffer(this._capacity),this._view=new _t(this._buffer)}get capacity(){return this._capacity}get size(){return this._size}get view(){return this._view}addInstance(){this._size+1>this._capacity&&this._grow();const e=this._findSlot();return this._view.state.set(e,1),this._size++,this.events.emit("instances-changed"),e}removeInstance(e){const a=this._view.state;F(e>=0&&e<this._capacity&&!!(1&a.get(e)),"invalid instance handle"),this._getStateFlag(e,18)?this._setStateFlags(e,32):this.freeInstance(e),this.events.emit("instances-changed")}freeInstance(e){const a=this._view.state;F(e>=0&&e<this._capacity&&!!(1&a.get(e)),"invalid instance handle"),a.set(e,0),this._size--}setLocalTransform(e,a,o=!0){this._view.localTransform.setMat(e,a),o&&this.updateModelTransform(e)}getLocalTransform(e,a){this._view.localTransform.getMat(e,a)}setGlobalTransform(e,a,o=!0){this._view.globalTransform.setMat(e,a),o&&this.updateModelTransform(e)}getGlobalTransform(e,a){this._view.globalTransform.getMat(e,a)}updateModelTransform(e){const a=this._view,o=M,r=z;a.localTransform.getMat(e,Ct),a.globalTransform.getMat(e,Ge);const n=Ia(Ge,Ge,Ct);se(o,n[12],n[13],n[14]),a.modelOrigin.setVec(e,o),Da(r,n),a.model.setMat(e,r);const l=Wo(M,n);l.sort(),a.modelScaleFactors.set(e,0,l[1]),a.modelScaleFactors.set(e,1,l[2]),$a(r,r),za(r,r),a.modelNormal.setMat(e,r),this._setStateFlags(e,64),this.events.emit("instance-transform-changed",{index:e})}getModelTransform(e,a){const o=this._view;o.model.getMat(e,z),o.modelOrigin.getVec(e,M),a[0]=z[0],a[1]=z[1],a[2]=z[2],a[3]=0,a[4]=z[3],a[5]=z[4],a[6]=z[5],a[7]=0,a[8]=z[6],a[9]=z[7],a[10]=z[8],a[11]=0,a[12]=M[0],a[13]=M[1],a[14]=M[2],a[15]=1}applyShaderTransformation(e,a){this.shaderTransformation!=null&&this.shaderTransformation.applyTransform(this,e,a)}getCombinedModelTransform(e,a){return this.getModelTransform(e,a),this.shaderTransformation!=null&&this.shaderTransformation.applyTransform(this,e,a),a}getCombinedLocalTransform(e,a){this._view.localTransform.getMat(e,a),this.shaderTransformation!=null&&this.shaderTransformation.applyTransform(this,e,a)}getCombinedMaxScaleFactor(e){let a=this._view.modelScaleFactors.get(e,1);return this.shaderTransformation!=null&&(this.shaderTransformation.scaleFactor(M,this,e),a*=Math.max(M[0],M[1],M[2])),a}getCombinedMedianScaleFactor(e){let a=this._view.modelScaleFactors.get(e,0);return this.shaderTransformation!=null&&(this.shaderTransformation.scaleFactor(M,this,e),a*=Ho(M[0],M[1],M[2])),a}getModel(e,a){this._view.model.getMat(e,a)}setFeatureAttribute(e,a){this._view.featureAttribute?.setVec(e,a)}getFeatureAttribute(e,a){this._view.featureAttribute?.getVec(e,a)}setColor(e,a){this._view.color?.setVec(e,a)}setObjectAndLayerIdColor(e,a){this._view.olidColor?.setVec(e,a)}setVisible(e,a){a!==this.getVisible(e)&&(this._setStateFlag(e,4,a),this.events.emit("instance-visibility-changed",{index:e}))}getVisible(e){return this._getStateFlag(e,4)}setHighlight(e,a){const{_highlightOptionsMap:o}=this,r=o.get(e);a?a!==r&&(o.set(e,a),this._setStateFlag(e,8,!0),this.events.emit("instance-highlight-changed")):r&&(o.delete(e),this._setStateFlag(e,8,!1),this.events.emit("instance-highlight-changed"))}get highlightOptionsMap(){return this._highlightOptionsMap}getHighlightStateFlag(e){return this._getStateFlag(e,8)}geHighlightOptionsPrev(e){const a=this._highlightOptionsMapPrev.get(e)??null;return this._highlightOptionsMapPrev.delete(e),a}getHighlightName(e){const a=this.highlightOptionsMap.get(e)??null;return a?this._highlightOptionsMapPrev.set(e,a):this._highlightOptionsMapPrev.delete(e),a}getState(e){return this._view.state.get(e)}getLodLevel(e){return this._view.lodLevel.get(e)}countFlags(e){let a=0;for(let o=0;o<this._capacity;++o)this.getState(o)&e&&++a;return a}_setStateFlags(e,a){const o=this._view.state;a=o.get(e)|a,o.set(e,a)}_clearStateFlags(e,a){const o=this._view.state;a=o.get(e)&~a,o.set(e,a)}_setStateFlag(e,a,o){o?this._setStateFlags(e,a):this._clearStateFlags(e,a)}_getStateFlag(e,a){return!!(this._view.state.get(e)&a)}_grow(){this._capacity=Math.max(_e,Math.floor(this._capacity*Wt)),this._buffer=this._layout.createBuffer(this._capacity).copyFrom(this._buffer),this._view=new _t(this._buffer)}_findSlot(){const e=this._view.state;let a=this._next;for(;1&e.get(a);)a=a+1===this._capacity?0:a+1;return this._next=a+1===this._capacity?0:a+1,a}};function Ho(t,e,a){return Math.max(Math.min(t,e),Math.min(Math.max(t,e),a))}c([N({constructOnly:!0})],oe.prototype,"shaderTransformation",void 0),c([N()],oe.prototype,"_size",void 0),c([N({readOnly:!0})],oe.prototype,"size",null),oe=c([j("esri.views.3d.webgl-engine.lib.lodRendering.InstanceData")],oe);const Uo=it().mat4f64("localTransform").mat4f64("globalTransform").vec4f64("boundingSphere").vec3f64("modelOrigin").mat3f("instanceModel").mat3f("instanceModelNormal").vec2f("modelScaleFactors");function ko(t){return ea(Uo.clone(),t).u8("state").u8("lodLevel")}function ea(t,e){return e.instancedFeatureAttribute&&t.vec4f("instanceFeatureAttribute"),e.instancedColor&&t.vec4u8("instanceColor"),Ht()&&t.vec4u8("instanceOlidColor"),t}const M=w(),z=he(),Ct=Bt(),Ge=Bt(),_e=64;let qo=class{constructor(e){this.model=e.instanceModel,this.modelNormal=e.instanceModelNormal,this.modelOriginHi=e.instanceModelOriginHi,this.modelOriginLo=e.instanceModelOriginLo,this.featureAttribute=e.getField("instanceFeatureAttribute",Kt),this.color=e.getField("instanceColor",Te),this.olidColor=e.getField("instanceOlidColor",Te)}},Qr=class{constructor(e,a){this._rctx=e,this._layout=a,this._headIndex=0,this._tailIndex=0,this._firstIndex=null,this._captureFirstIndex=!0,this._updating=!1,this._prevHeadIndex=0,this._resized=!1,this._capacity=1}destroy(){this._buffer&&this._buffer.destroy()}get buffer(){return this._buffer.buffer}get view(){return this._view}get capacity(){return this._capacity}get size(){const e=this._headIndex,a=this._tailIndex;return e>=a?e-a:e+this._capacity-a}get isEmpty(){return this._headIndex===this._tailIndex}get isFull(){return this._tailIndex===(this._headIndex+1)%this._capacity}get headIndex(){return this._headIndex}get tailIndex(){return this._tailIndex}get firstIndex(){return this._firstIndex}get usedMemory(){return this._buffer?.usedMemory??0}reset(){this._headIndex=0,this._tailIndex=0,this._firstIndex=null}startUpdateCycle(){this._captureFirstIndex=!0}beginUpdate(){F(!this._updating,"already updating"),this._updating=!0,this._prevHeadIndex=this._headIndex}endUpdate(){F(this._updating,"not updating"),this.size<Pa*this.capacity&&this._shrink(),this._resized?(this._buffer.transferAll(),this._resized=!1):this._transferRange(this._prevHeadIndex,this._headIndex),this._updating=!1}allocateHead(){F(this._updating,"not updating"),this.isFull&&this._grow();const e=this.headIndex;return this._captureFirstIndex&&(this._firstIndex=e,this._captureFirstIndex=!1),this._incrementHead(),F(this._headIndex!==this._tailIndex,"invalid pointers"),e}freeTail(){F(this._updating,"not updating"),F(this.size>0,"invalid size");const e=this._tailIndex===this._firstIndex;this._incrementTail(),e&&(this._firstIndex=this._tailIndex)}_grow(){const e=Math.max(_e,Math.floor(this._capacity*Wt));this._resize(e)}_shrink(){const e=Math.max(_e,Math.floor(this._capacity*Fa));this._resize(e)}_resize(e){if(F(this._updating,"not updating"),e===this._capacity)return;const a=new Ao(this._rctx,this._layout,e);if(this._buffer){this._firstIndex&&(this._firstIndex=(this._firstIndex+this._capacity-this._tailIndex)%this._capacity);const o=this.size,r=this._compactInstances(a);F(r===o,"invalid compaction"),this._buffer.destroy(),this._tailIndex=0,this._headIndex=r,this._prevHeadIndex=0}this._resized=!0,this._capacity=e,this._buffer=a,this._view=new qo(this._layout.createView(this._buffer.array))}_compactInstances(e){const a=this._headIndex,o=this._tailIndex;return o<a?(this._buffer.copyRange(o,a,e),a-o):o>a?(this._buffer.copyRange(o,this._capacity,e),a>0&&this._buffer.copyRange(0,a,e,this._capacity-o),a+(this._capacity-o)):0}_incrementHead(e=1){this._headIndex=(this._headIndex+e)%this._capacity}_incrementTail(e=1){this._tailIndex=(this._tailIndex+e)%this._capacity}_transferRange(e,a){e<a?this._buffer.transferRange(e,a):e>a&&(a>0&&this._buffer.transferRange(0,a),this._buffer.transferRange(e,this._capacity))}};const Zo=it().vec3f("instanceModelOriginHi").vec3f("instanceModelOriginLo").mat3f("instanceModel").mat3f("instanceModelNormal");function Jo(t){return ea(Zo.clone(),t)}function tn({normalTexture:t,metallicRoughnessTexture:e,metallicFactor:a,roughnessFactor:o,emissiveTexture:r,emissiveFactor:n,occlusionTexture:l}){return t==null&&e==null&&r==null&&(n==null||Gt(n,Xe))&&l==null&&(o==null||o===1)&&(a==null||a===1)}const Yo=le(1,1,.5),an=le(0,.6,.2),on=le(0,1,.2);function ta(t){t.vertex.code.add(i`vec4 offsetBackfacingClipPosition(vec4 posClip, vec3 posWorld, vec3 normalWorld, vec3 camPosWorld) {
vec3 camToVert = posWorld - camPosWorld;
bool isBackface = dot(camToVert, normalWorld) > 0.0;
if (isBackface) {
posClip.z += 0.0000003 * posClip.w;
}
return posClip;
}`)}function Xo(t){t.varyings.add("linearDepth","float",{invariant:!0})}function Ko(t,e){Xo(t),t.vertex.code.add(i`
    void forwardLinearDepth(float _linearDepth) { ${f(e,"linearDepth = _linearDepth;")} }
  `)}const It=he();function aa(t,e){const{hasModelTransformation:a,instancedDoublePrecision:o,instanced:r,output:n,hasVertexTangents:l}=e;a&&(t.vertex.uniforms.add(new eo("model",d=>d.modelTransformation??bt)),t.vertex.uniforms.add(new B("normalLocalOriginFromModel",d=>(ja(It,d.modelTransformation??bt),It)))),r&&o&&(t.attributes.add("instanceModelOriginHi","vec3"),t.attributes.add("instanceModelOriginLo","vec3"),t.attributes.add("instanceModel","mat3"),t.attributes.add("instanceModelNormal","mat3"));const s=t.vertex;o&&(s.include(Go),s.uniforms.add(new we("viewOriginHi",d=>oo(se(ve,d.camera.viewInverseTransposeMatrix[3],d.camera.viewInverseTransposeMatrix[7],d.camera.viewInverseTransposeMatrix[11]),ve)),new we("viewOriginLo",d=>io(se(ve,d.camera.viewInverseTransposeMatrix[3],d.camera.viewInverseTransposeMatrix[7],d.camera.viewInverseTransposeMatrix[11]),ve)))),s.code.add(i`
    vec3 getVertexInLocalOriginSpace() {
      return ${a?o?"(model * vec4(instanceModel * localPosition().xyz, 1.0)).xyz":"(model * localPosition()).xyz":o?"instanceModel * localPosition().xyz":"localPosition().xyz"};
    }

    vec3 subtractOrigin(vec3 _pos) {
      ${o?i`
          // Issue: (should be resolved now with invariant position) https://devtopia.esri.com/WebGIS/arcgis-js-api/issues/56280
          vec3 originDelta = dpAdd(viewOriginHi, viewOriginLo, -instanceModelOriginHi, -instanceModelOriginLo);
          return _pos - originDelta;`:"return vpos;"}
    }
    `),s.code.add(i`
    vec3 dpNormal(vec4 _normal) {
      return normalize(${a?o?"normalLocalOriginFromModel * (instanceModelNormal * _normal.xyz)":"normalLocalOriginFromModel * _normal.xyz":o?"instanceModelNormal * _normal.xyz":"_normal.xyz"});
    }
    `),n===4&&(ro(s),s.code.add(i`
    vec3 dpNormalView(vec4 _normal) {
      return normalize((viewNormal * ${a?o?"vec4(normalLocalOriginFromModel * (instanceModelNormal * _normal.xyz), 1.0)":"vec4(normalLocalOriginFromModel * _normal.xyz, 1.0)":o?"vec4(instanceModelNormal * _normal.xyz, 1.0)":"_normal"}).xyz);
    }
    `)),l&&s.code.add(i`
    vec4 dpTransformVertexTangent(vec4 _tangent) {
      ${a?o?"return vec4(normalLocalOriginFromModel * (instanceModelNormal * _tangent.xyz), _tangent.w);":"return vec4(normalLocalOriginFromModel * _tangent.xyz, _tangent.w);":o?"return vec4(instanceModelNormal * _tangent.xyz, _tangent.w);":"return _tangent;"}
    }`)}const ve=w();function Qo(t,e){e.instancedColor?(t.attributes.add("instanceColor","vec4"),t.vertex.include(et),t.vertex.include(Ut),t.vertex.include(kt),t.vertex.code.add(i`
      MaskedColor applyInstanceColor(MaskedColor color) {
        return multiplyMaskedColors( color, createMaskedFromUInt8NaNColor(${"instanceColor"}));
      }
    `)):t.vertex.code.add(i`MaskedColor applyInstanceColor(MaskedColor color) {
return color;
}`)}function ei(t,e){t.varyings.add("colorMixMode","int"),t.varyings.add("opacityMixMode","int"),t.vertex.uniforms.add(new zo("symbolColorMixMode",a=>ye[a.colorMixMode])),e.hasSymbolColors?(t.vertex.include(et),t.vertex.include(Ut),t.vertex.include(kt),t.attributes.add("symbolColor","vec4"),t.vertex.code.add(i`
    MaskedColor applySymbolColor(MaskedColor color) {
      return multiplyMaskedColors(color, createMaskedFromUInt8NaNColor(${"symbolColor"}));
    }
  `)):t.vertex.code.add(i`MaskedColor applySymbolColor(MaskedColor color) {
return color;
}`),t.vertex.code.add(i`
    void forwardColorMixMode(bvec4 mask) {
      colorMixMode = mask.r ? ${i.int(ye.ignore)} : symbolColorMixMode;
      opacityMixMode = mask.a ? ${i.int(ye.ignore)} : symbolColorMixMode;
    }
  `)}function rt(t,e){const{vertex:a,fragment:o,varyings:r}=t;t.include(Qo,e),t.include(ei,e),t.include(So,e),a.include(et),a.include(no),a.include(qt),a.uniforms.add(new so("externalColor",n=>n.externalColor,{supportsNaN:!0})),r.add("vExternalColor","vec4"),a.code.add(i`
    void forwardDefaultMaterialExternalColor() {
      forwardVertexColor();

      MaskedColor maskedColor =
        applySymbolColor(applyVVColor(applyInstanceColor(createMaskedFromNaNColor(externalColor))));

      vExternalColor = maskedColor.color;
      forwardColorMixMode(maskedColor.mask);
    }

    bool shouldCullByOpacity() {
      return opacityMixMode != ${i.int(ye.ignore)} && vExternalColor.a < alphaCutoff;
    }
  `),o.include(Po),o.uniforms.add(new x("opacity",n=>n.opacity),new x("layerOpacity",n=>n.layerOpacity)),o.code.add(i`
    float getDefaultMaterialOpacity(float textureOpacity) {
      return layerOpacity * mixExternalOpacity(
        ${f(e.hasVertexColors,"vColor.a * ")} opacity,
        textureOpacity,
        vExternalColor.a,
        opacityMixMode
      );
    }
  `)}function ti(t,e){switch(e.output){case 5:case 6:case 7:case 8:t.fragment.code.add(i`void outputDepth(float _linearDepth){
const float slope_scale = 2.0;
const float bias = 20.0 * .000015259;
float m = max(abs(dFdx(_linearDepth)), abs(dFdy(_linearDepth)));
gl_FragDepth = _linearDepth + slope_scale * m + bias;
}`)}}function Y(t,e){ai(t,e,new x("textureAlphaCutoff",a=>a.textureAlphaCutoff))}function ai(t,e,a){const o=t.fragment;switch(o.code.add("void discardOrAdjustAlpha(inout vec4 color) {"),e.alphaDiscardMode){case 1:o.code.add("color.a = 1.0;");break;case 0:e.output!==0&&(o.include(qt),o.code.add("if (color.a < alphaCutoff) discard;"));break;case 3:e.output!==0&&o.uniforms.add(a).code.add("if (color.a < textureAlphaCutoff) discard;");break;case 2:o.uniforms.add(a).code.add(`
        if (color.a < textureAlphaCutoff) discard;
        color.a = 1.0;
      `);break;case 4:break;default:e.alphaDiscardMode}o.code.add("}")}function oa(t,e){const{vertex:a,fragment:o,varyings:r}=t,{hasColorTexture:n,alphaDiscardMode:l}=e,s=n&&l!==1,{output:d,normalType:u,hasColorTextureTransform:g}=e;switch(d){case 3:k(a,e),t.include(Z),o.include(q,e),t.include(U,e),s&&o.uniforms.add(new b("tex",h=>h.texture)),a.main.add(i`vpos = getVertexInLocalOriginSpace();
vpos = subtractOrigin(vpos);
vpos = addVerticalOffset(vpos, localOrigin);
gl_Position = transformPosition(proj, view, vpos);
forwardTextureCoordinates();`),t.include(Y,e),o.main.add(i`
        discardBySlice(vpos);
        ${f(s,i`vec4 texColor = texture(tex, ${g?"colorUV":"vuv0"});
                discardOrAdjustAlpha(texColor);`)}`);break;case 5:case 6:case 7:case 8:case 11:k(a,e),t.include(Z),t.include(U,e),t.include(ne,e),t.include(ti,e),o.include(q,e),t.include(To,e),_o(t),r.add("depth","float",{invariant:!0}),s&&o.uniforms.add(new b("tex",h=>h.texture)),a.main.add(i`vpos = getVertexInLocalOriginSpace();
vpos = subtractOrigin(vpos);
vpos = addVerticalOffset(vpos, localOrigin);
gl_Position = transformPositionWithDepth(proj, view, vpos, nearFar, depth);
forwardTextureCoordinates();
forwardObjectAndLayerIdColor();`),t.include(Y,e),o.main.add(i`
        discardBySlice(vpos);
        ${f(s,i`vec4 texColor = texture(tex, ${g?"colorUV":"vuv0"});
               discardOrAdjustAlpha(texColor);`)}
        ${d===11?i`outputObjectAndLayerIdColor();`:i`outputDepth(depth);`}`);break;case 4:{k(a,e),t.include(Z),t.include(Pe,e),t.include(Qt,e),t.include(U,e),t.include(ne,e),t.include(rt,e),s&&o.uniforms.add(new b("tex",v=>v.texture)),u===2&&r.add("vPositionView","vec3",{invariant:!0});const h=u===0||u===1;a.main.add(i`
        forwardDefaultMaterialExternalColor();

        if (shouldCullByOpacity()) {
          gl_Position = vec4(1e38, 1e38, 1e38, 1.0);
          return;
        }

        vpos = getVertexInLocalOriginSpace();
        ${h?i`vNormalWorld = dpNormalView(vvLocalNormal(normalModel()));`:i`vPositionView = (view * vec4(vpos, 1.0)).xyz;`}
        vpos = subtractOrigin(vpos);
        vpos = addVerticalOffset(vpos, localOrigin);
        gl_Position = transformPosition(proj, view, vpos);
        forwardTextureCoordinates();`),o.include(q,e),t.include(Y,e),o.main.add(i`
        discardBySlice(vpos);
        ${f(s,i`vec4 texColor = texture(tex, ${g?"colorUV":"vuv0"});
               discardOrAdjustAlpha(texColor);`,i`vec4 texColor = vec4(1.0);`)}
        float opacity = getDefaultMaterialOpacity(texColor.a);

        ${u===2?i`vec3 normal = screenDerivativeNormal(vPositionView);`:i`vec3 normal = normalize(vNormalWorld);
                    if (gl_FrontFacing == false){
                      normal = -normal;
                    }`}
        fragColor = vec4(0.5 + 0.5 * normal, opacity);`);break}case 10:k(a,e),t.include(Z),t.include(U,e),t.include(ne,e),s&&o.uniforms.add(new b("tex",h=>h.texture)),a.main.add(i`vpos = getVertexInLocalOriginSpace();
vpos = subtractOrigin(vpos);
vpos = addVerticalOffset(vpos, localOrigin);
gl_Position = transformPosition(proj, view, vpos);
forwardTextureCoordinates();`),o.include(q,e),t.include(Y,e),t.include(lo,e),o.main.add(i`
        discardBySlice(vpos);
        ${f(s,i`vec4 texColor = texture(tex, ${g?"colorUV":"vuv0"});
                discardOrAdjustAlpha(texColor);`)}
        calculateOcclusionAndOutputHighlight();`)}}function oi(t,e){return ii(t,e)}function ii(t,e){const a=t.fragment,{hasVertexTangents:o,doubleSidedMode:r,hasNormalTexture:n,textureCoordinateType:l,bindType:s,hasNormalTextureTransform:d}=e;o?(t.attributes.add("tangent","vec4"),t.varyings.add("vTangent","vec4"),r===2?a.code.add(i`mat3 computeTangentSpace(vec3 normal) {
float tangentHeadedness = gl_FrontFacing ? vTangent.w : -vTangent.w;
vec3 tangent = normalize(gl_FrontFacing ? vTangent.xyz : -vTangent.xyz);
vec3 bitangent = cross(normal, tangent) * tangentHeadedness;
return mat3(tangent, bitangent, normal);
}`):a.code.add(i`mat3 computeTangentSpace(vec3 normal) {
float tangentHeadedness = vTangent.w;
vec3 tangent = normalize(vTangent.xyz);
vec3 bitangent = cross(normal, tangent) * tangentHeadedness;
return mat3(tangent, bitangent, normal);
}`)):a.code.add(i`mat3 computeTangentSpace(vec3 normal, vec3 pos, vec2 st) {
vec3 Q1 = dFdx(pos);
vec3 Q2 = dFdy(pos);
vec2 stx = dFdx(st);
vec2 sty = dFdy(st);
float det = stx.t * sty.s - sty.t * stx.s;
vec3 T = stx.t * Q2 - sty.t * Q1;
T = T - normal * dot(normal, T);
T *= inversesqrt(max(dot(T,T), 1.e-10));
vec3 B = sign(det) * cross(normal, T);
return mat3(T, B, normal);
}`),n&&l!==0&&(t.include(Lt,e),a.uniforms.add(s===1?new b("normalTexture",u=>u.textureNormal):new W("normalTexture",u=>u.textureNormal)),d&&(a.uniforms.add(s===1?new Yt("scale",u=>u.scale??yt):new at("scale",u=>u.scale??yt)),a.uniforms.add(new B("normalTextureTransformMatrix",u=>u.normalTextureTransformMatrix??K))),a.code.add(i`vec3 computeTextureNormal(mat3 tangentSpace, vec2 uv) {
vec3 rawNormal = textureLookup(normalTexture, uv).rgb * 2.0 - 1.0;`),d&&a.code.add(i`mat3 normalRotation = mat3(normalTextureTransformMatrix[0][0]/scale[0], normalTextureTransformMatrix[0][1]/scale[1], 0.0,
normalTextureTransformMatrix[1][0]/scale[0], normalTextureTransformMatrix[1][1]/scale[1], 0.0,
0.0, 0.0, 0.0 );
rawNormal.xy = (normalRotation * vec3(rawNormal.x, rawNormal.y, 1.0)).xy;`),a.code.add(i`return tangentSpace * rawNormal;
}`))}const ri=3e5,Dt=5e5,$t=16;function ia(){const t=new V,e=t.fragment;return t.include(me),t.include(ze),e.include(Q),e.include(Ie),e.uniforms.add(new R("radius",a=>Fe(a.camera))).code.add(i`vec3 sphere[16] = vec3[16](
vec3(0.186937, 0.0, 0.0),
vec3(0.700542, 0.0, 0.0),
vec3(-0.864858, -0.481795, -0.111713),
vec3(-0.624773, 0.102853, -0.730153),
vec3(-0.387172, 0.260319, 0.007229),
vec3(-0.222367, -0.642631, -0.707697),
vec3(-0.01336, -0.014956, 0.169662),
vec3(0.122575, 0.1544, -0.456944),
vec3(-0.177141, 0.85997, -0.42346),
vec3(-0.131631, 0.814545, 0.524355),
vec3(-0.779469, 0.007991, 0.624833),
vec3(0.308092, 0.209288,0.35969),
vec3(0.359331, -0.184533, -0.377458),
vec3(0.192633, -0.482999, -0.065284),
vec3(0.233538, 0.293706, -0.055139),
vec3(0.417709, -0.386701, 0.442449)
);
float fallOffFunction(float vv, float vn, float bias) {
float f = max(radius * radius - vv, 0.0);
return f * f * f * max(vn - bias, 0.0);
}`),e.code.add(i`float aoValueFromPositionsAndNormal(vec3 C, vec3 n_C, vec3 Q) {
vec3 v = Q - C;
float vv = dot(v, v);
float vn = dot(normalize(v), n_C);
return fallOffFunction(vv, vn, 0.1);
}`),t.outputs.add("fragOcclusion","float"),e.uniforms.add(new b("normalMap",a=>a.normalTexture),new b("depthMap",a=>a.depthTexture),new x("projScale",a=>a.projScale),new b("rnm",a=>a.noiseTexture),new Yt("rnmScale",(a,o)=>X(zt,o.camera.fullWidth/a.noiseTexture.descriptor.width,o.camera.fullHeight/a.noiseTexture.descriptor.height)),new x("intensity",a=>a.intensity),new tt("screenSize",a=>X(zt,a.camera.fullWidth,a.camera.fullHeight))).main.add(i`
    float depth = depthFromTexture(depthMap, uv);

    // Early out if depth is out of range, such as in the sky
    if (depth >= 1.0 || depth <= 0.0) {
      fragOcclusion = 1.0;
      return;
    }

    // get the normal of current fragment
    ivec2 iuv = ivec2(uv * vec2(textureSize(normalMap, 0)));
    vec4 norm4 = texelFetch(normalMap, iuv, 0);
    if (norm4.a == 0.0) {
      fragOcclusion = 1.0;
      return;
    }
    vec3 norm = normalize(norm4.xyz * 2.0 - 1.0);

    float currentPixelDepth = linearizeDepth(depth);
    vec3 currentPixelPos = reconstructPosition(gl_FragCoord.xy, currentPixelDepth);

    float sum = 0.0;
    vec3 tapPixelPos;

    vec3 fres = normalize(2.0 * texture(rnm, uv * rnmScale).xyz - 1.0);

    // note: the factor 2.0 should not be necessary, but makes ssao much nicer.
    // bug or deviation from CE somewhere else?
    float ps = projScale / (2.0 * currentPixelPos.z * zScale.x + zScale.y);

    for(int i = 0; i < ${i.int($t)}; ++i) {
      vec2 unitOffset = reflect(sphere[i], fres).xy;
      vec2 offset = vec2(-unitOffset * radius * ps);

      // don't use current or very nearby samples
      if( abs(offset.x) < 2.0 || abs(offset.y) < 2.0){
        continue;
      }

      vec2 tc = vec2(gl_FragCoord.xy + offset);
      if (tc.x < 0.0 || tc.y < 0.0 || tc.x > screenSize.x || tc.y > screenSize.y) continue;
      vec2 tcTap = tc / screenSize;
      float occluderFragmentDepth = linearDepthFromTexture(depthMap, tcTap);

      tapPixelPos = reconstructPosition(tc, occluderFragmentDepth);

      sum += aoValueFromPositionsAndNormal(currentPixelPos, norm, tapPixelPos);
    }

    // output the result
    float A = max(1.0 - sum * intensity / float(${i.int($t)}), 0.0);

    // Anti-tone map to reduce contrast and drag dark region farther: (x^0.2 + 1.2 * x^4) / 2.2
    A = (pow(A, 0.2) + 1.2 * pow(A, 4.0)) * INV_GAMMA;

    fragOcclusion = A;
  `),t}function Fe(t){return Math.max(10,20*t.computeScreenPixelSizeAtDist(Math.abs(4*t.relativeElevation)))}const zt=Ke(),ni=Object.freeze(Object.defineProperty({__proto__:null,build:ia,getRadius:Fe},Symbol.toStringTag,{value:"Module"})),Ne=4;function ra(){const t=new V,e=t.fragment;t.include(me);const a=(Ne+1)/2,o=1/(2*a*a);return e.include(Q),e.uniforms.add(new b("depthMap",r=>r.depthTexture),new W("tex",r=>r.colorTexture),new at("blurSize",r=>r.blurSize),new x("projScale",(r,n)=>{const l=n.camera.distance;return l>5e4?Math.max(0,r.projScale-(l-5e4)):r.projScale})),e.code.add(i`
    void blurFunction(vec2 uv, float r, float center_d, float sharpness, inout float wTotal, inout float bTotal) {
      float c = texture(tex, uv).r;
      float d = linearDepthFromTexture(depthMap, uv);

      float ddiff = d - center_d;

      float w = exp(-r * r * ${i.float(o)} - ddiff * ddiff * sharpness);
      wTotal += w;
      bTotal += w * c;
    }
  `),t.outputs.add("fragBlur","float"),e.main.add(i`
    float b = 0.0;
    float w_total = 0.0;

    float center_d = linearDepthFromTexture(depthMap, uv);

    float sharpness = -0.05 * projScale / center_d;
    for (int r = -${i.int(Ne)}; r <= ${i.int(Ne)}; ++r) {
      float rf = float(r);
      vec2 uvOffset = uv + rf * blurSize;
      blurFunction(uvOffset, rf, center_d, sharpness, w_total, b);
    }
    fragBlur = b / w_total;`),t}const si=Object.freeze(Object.defineProperty({__proto__:null,build:ra},Symbol.toStringTag,{value:"Module"}));let He=class extends ee{constructor(){super(...arguments),this.shader=new L(si,()=>E(()=>Promise.resolve().then(()=>_r),void 0))}initializePipeline(){return te({colorWrite:ae})}};He=c([j("esri.views.3d.webgl-engine.effects.ssao.SSAOBlurTechnique")],He);const li="eXKEvZaUc66cjIKElE1jlJ6MjJ6Ufkl+jn2fcXp5jBx7c6KEflSGiXuXeW6OWs+tfqZ2Yot2Y7Zzfo2BhniEj3xoiXuXj4eGZpqEaHKDWjSMe7palFlzc3BziYOGlFVzg6Zzg7CUY5JrjFF7eYJ4jIKEcyyEonSXe7qUfqZ7j3xofqZ2c4R5lFZ5Y0WUbppoe1l2cIh2ezyUho+BcHN2cG6DbpqJhqp2e1GcezhrdldzjFGUcyxjc3aRjDyEc1h7Sl17c6aMjH92pb6Mjpd4dnqBjMOEhqZleIOBYzB7gYx+fnqGjJuEkWlwnCx7fGl+c4hjfGyRe5qMlNOMfnqGhIWHc6OMi4GDc6aMfqZuc6aMzqJzlKZ+lJ6Me3qRfoFue0WUhoR5UraEa6qMkXiPjMOMlJOGe7JrUqKMjK6MeYRzdod+Sl17boiPc6qEeYBlcIh2c1WEe7GDiWCDa0WMjEmMdod+Y0WcdntzhmN8WjyMjKJjiXtzgYxYaGd+a89zlEV7e2GJfnd+lF1rcK5zc4p5cHuBhL6EcXp5eYB7fnh8iX6HjIKEeaxuiYOGc66RfG2Ja5hzjlGMjEmMe9OEgXuPfHyGhPeEdl6JY02McGuMfnqGhFiMa3WJfnx2l4hwcG1uhmN8c0WMc39og1GBbrCEjE2EZY+JcIh2cIuGhIWHe0mEhIVrc09+gY5+eYBlnCyMhGCDl3drfmmMgX15aGd+gYx+fnuRfnhzY1SMsluJfnd+hm98WtNrcIuGh4SEj0qPdkqOjFF7jNNjdnqBgaqUjMt7boeBhnZ4jDR7c5pze4GGjEFrhLqMjHyMc0mUhKZze4WEa117kWlwbpqJjHZ2eX2Bc09zeId+e0V7WlF7jHJ2l72BfId8l3eBgXyBe897jGl7c66cgW+Xc76EjKNbgaSEjGx4fId8jFFjgZB8cG6DhlFziZhrcIh2fH6HgUqBgXiPY8dahGFzjEmMhEFre2dxhoBzc5SGfleGe6alc7aUeYBlhKqUdlp+cH5za4OEczxza0Gcc4J2jHZ5iXuXjH2Jh5yRjH2JcFx+hImBjH+MpddCl3dreZeJjIt8ZW18bm1zjoSEeIOBlF9oh3N7hlqBY4+UeYFwhLJjeYFwaGd+gUqBYxiEYot2fqZ2ondzhL6EYyiEY02Ea0VjgZB8doaGjHxoc66cjEGEiXuXiXWMiZhreHx8frGMe75rY02Ec5pzfnhzlEp4a3VzjM+EhFFza3mUY7Zza1V5e2iMfGyRcziEhDyEkXZ2Y4OBnCx7g5t2eyBjgV6EhEFrcIh2dod+c4Z+nJ5zjm15jEmUeYxijJp7nL6clIpjhoR5WrZraGd+fnuRa6pzlIiMg6ZzfHx5foh+eX1ufnB5eX1ufnB5aJt7UqKMjIh+e3aBfm5lbYSBhGFze6J4c39oc0mUc4Z+e0V7fKFVe0WEdoaGY02Ec4Z+Y02EZYWBfH6HgU1+gY5+hIWUgW+XjJ57ebWRhFVScHuBfJ6PhBx7WqJzlM+Ujpd4gHZziX6HjHmEgZN+lJt5boiPe2GJgX+GjIGJgHZzeaxufnB5hF2JtdN7jJ57hp57hK6ElFVzg6ZzbmiEbndzhIWHe3uJfoFue3qRhJd2j3xoc65zlE1jc3p8lE1jhniEgXJ7e657vZaUc3qBh52BhIF4aHKDa9drgY5+c52GWqZzbpqJe8tjnM+UhIeMfo2BfGl+hG1zSmmMjKJjZVaGgX15c1lze0mEp4OHa3mUhIWHhDyclJ6MeYOJkXiPc0VzhFiMlKaEboSJa5Jze41re3qRhn+HZYWBe0mEc4p5fnORbox5lEp4hGFjhGGEjJuEc1WEhLZjeHeGa7KlfHx2hLaMeX1ugY5+hIWHhKGPjMN7c1WEho1zhoBzZYx7fnhzlJt5exyUhFFziXtzfmmMa6qMYyiEiXxweV12kZSMeWqXSl17fnhzxmmMrVGEe1mcc4p5eHeGjK6MgY5+doaGa6pzlGV7g1qBh4KHkXiPeW6OaKqafqZ2eXZ5e1V7jGd7boSJc3BzhJd2e0mcYot2h1RoY8dahK6EQmWEWjx7e1l2lL6UgXyBdnR4eU9zc0VreX1umqaBhld7fo2Bc6KEc5Z+hDyEcIeBWtNrfHyGe5qMhMuMe5qMhEGEbVVupcNzg3aHhIF4boeBe0mEdlptc39ofFl5Y8uUlJOGiYt2UmGEcyxjjGx4jFF7a657ZYWBnElzhp57iXtrgZN+tfOEhIOBjE2HgU1+e8tjjKNbiWCDhE15gUqBgYN7fnqGc66ce9d7iYSBj0qPcG6DnGGcT3eGa6qMZY+JlIiMl4hwc3aRdnqBlGV7eHJ2hLZjfnuRhDyEeX6MSk17g6Z+c6aUjHmEhIF4gXyBc76EZW18fGl+fkl+jCxrhoVwhDyUhIqGlL2DlI6EhJd2tdN7eYORhEGMa2Faa6pzc3Bzc4R5lIRznM+UY9eMhDycc5Z+c4p5c4iGY117pb6MgXuPrbJafnx2eYOJeXZ5e657hDyEcziElKZjfoB5eHeGj4WRhGGEe6KGeX1utTStc76EhFGJnCyMa5hzfH6HnNeceYB7hmN8gYuMhIVrczSMgYF8h3N7c5pza5hzjJqEYIRdgYuMlL2DeYRzhGGEeX1uhLaEc4iGeZ1zdl6JhrVteX6Me2iMfm5lWqJzSpqEa6pzdnmchHx2c6OMhNdrhoR5g3aHczxzeW52gV6Ejm15frGMc0Vzc4Z+l3drfniJe+9rWq5rlF1rhGGEhoVwe9OEfoh+e7pac09+c3qBY0lrhDycdnp2lJ6MiYOGhGCDc3aRlL2DlJt5doaGdnp2gYF8gWeOjF2Uc4R5c5Z+jEmMe7KEc4mEeYJ4dmyBe0mcgXiPbqJ7eYB7fmGGiYSJjICGlF1reZ2PnElzbpqJfH6Hc39oe4WEc5eJhK6EhqyJc3qBgZB8c09+hEmEaHKDhFGJc5SGiXWMUpaEa89zc6OMnCyMiXtrho+Be5qMc7KEjJ57dmN+hKGPjICGbmiEe7prdod+hGCDdnmchBx7eX6MkXZ2hGGEa657hm98jFFjY5JreYOJgY2EjHZ2a295Y3FajJ6Mc1J+YzB7e4WBjF2Uc4R5eV12gYxzg1qBeId+c9OUc5pzjFFjgY5+hFiMlIaPhoR5lIpjjIKBlNdSe7KEeX2BfrGMhIqGc65zjE2UhK6EklZ+QmWEeziMWqZza3VzdnR4foh+gYF8n3iJiZhrnKp7gYF8eId+lJ6Me1lrcIuGjKJjhmN8c66MjFF7a6prjJ6UnJ5zezyUfruRWlF7nI5zfHyGe657h4SEe8tjhBx7jFFjc09+c39ojICMeZeJeXt+YzRzjHZ2c0WEcIeBeXZ5onSXkVR+gYJ+eYFwdldzgYF7eX2BjJ6UiXuXlE1jh4SEe1mchLJjc4Z+hqZ7eXZ5bm1zlL6Ue5p7iWeGhKqUY5pzjKJjcIeBe8t7gXyBYIRdlEp4a3mGnK6EfmmMZpqEfFl5gYxzjKZuhGFjhoKGhHx2fnx2eXuMe3aBiWeGvbKMe6KGa5hzYzB7gZOBlGV7hmN8hqZlYot2Y117a6pzc6KEfId8foB5rctrfneJfJ6PcHN2hFiMc5pzjH92c0VzgY2EcElzdmCBlFVzg1GBc65zY4OBboeBcHiBeYJ4ewxzfHx5lIRzlEmEnLKEbk1zfJ6PhmN8eYBljBiEnMOEiXxwezyUcIeBe76EdsKEeX2BdnR4jGWUrXWMjGd7fkl+j4WRlEGMa5Jzho+BhDyEfnqMeXt+g3aHlE1jczClhNN7ZW18eHx8hGFjZW18iXWMjKJjhH57gYuMcIuGWjyMe4ZtjJuExmmMj4WRdntzi4GDhFFzYIRdnGGcjJp7Y0F7e4WEkbCGiX57fnSHa657a6prhBCMe3Z+SmmMjH92eHJ2hK6EY1FzexhrvbKMnI5za4OEfnd+eXuMhImBe897hLaMjN+EfG+BeIOBhF1+eZeJi4GDkXZ2eXKEgZ6Ejpd4c2GHa1V5e5KUfqZuhCx7jKp7lLZrg11+hHx2hFWUoot2nI5zgbh5mo9zvZaUe3qRbqKMfqZ2kbCGhFiM";let ci=class extends A{constructor(){super(...arguments),this.projScale=1}},di=class extends ci{constructor(){super(...arguments),this.intensity=1}},ui=class extends A{},hi=class extends ui{constructor(){super(...arguments),this.blurSize=Ke()}},Ue=class extends ee{constructor(){super(...arguments),this.shader=new L(ni,()=>E(()=>Promise.resolve().then(()=>Cr),void 0))}initializePipeline(){return te({colorWrite:ae})}};Ue=c([j("esri.views.3d.webgl-engine.effects.ssao.SSAOTechnique")],Ue);const ie=2;let ge=class extends Xt{constructor(e){super(e),this.consumes={required:["normals"]},this.produces=J.AMBIENT_ILLUMINATION,this._enableTime=fe(0),this._passParameters=new di,this._drawParameters=new hi}initialize(){const e=Uint8Array.from(atob(li),o=>o.charCodeAt(0)),a=new Oa(32);a.wrapMode=33071,a.pixelFormat=6407,a.wrapMode=10497,a.hasMipmap=!0,this._passParameters.noiseTexture=new Ra(this.renderingContext,a,e),this.addHandles(Nt(()=>this.view.stage.renderer.hasAmbientIllumination,()=>this._enableTime=fe(0)))}destroy(){this._passParameters.noiseTexture=Wa(this._passParameters.noiseTexture)}render(e){const a=e.find(({name:Re})=>Re==="normals"),o=a?.getTexture(),r=a?.getTexture(Et);if(!o||!r)return;const n=this.techniques.getCompiled(Ue),l=this.techniques.getCompiled(He);if(!n||!l)return this._enableTime=fe(performance.now()),void this.requestRender(1);this._enableTime===0&&(this._enableTime=fe(performance.now()));const s=this.renderingContext,d=this.view.qualitySettings.fadeDuration,u=this.bindParameters,g=u.camera,h=g.relativeElevation,v=Ba((Dt-h)/(Dt-ri),0,1),C=d>0?Math.min(d,performance.now()-this._enableTime)/d:1,y=C*v;this._passParameters.normalTexture=o,this._passParameters.depthTexture=r,this._passParameters.projScale=1/g.computeScreenPixelSizeAtDist(1),this._passParameters.intensity=4*mi/Fe(g)**6*y;const I=g.fullViewport[2],S=g.fullViewport[3],P=this.fboCache.acquire(I,S,"ssao input",2);s.bindFramebuffer(P.fbo),s.setViewport(0,0,I,S),s.bindTechnique(n,u,this._passParameters,this._drawParameters),s.screen.draw();const $=Math.round(I/ie),T=Math.round(S/ie),_=this.fboCache.acquire($,T,"ssao blur",0);s.bindFramebuffer(_.fbo),this._drawParameters.colorTexture=P.getTexture(),X(this._drawParameters.blurSize,0,ie/S),s.bindTechnique(l,u,this._passParameters,this._drawParameters),s.setViewport(0,0,$,T),s.screen.draw(),P.release();const D=this.fboCache.acquire($,T,J.AMBIENT_ILLUMINATION,0);return s.bindFramebuffer(D.fbo),s.setViewport(0,0,I,S),s.setClearColor(1,1,1,0),s.clear(16384),this._drawParameters.colorTexture=_.getTexture(),X(this._drawParameters.blurSize,ie/I,0),s.bindTechnique(l,u,this._passParameters,this._drawParameters),s.setViewport(0,0,$,T),s.screen.draw(),s.setViewport4fv(g.fullViewport),_.release(),C<1&&this.requestRender(2),D}};c([N()],ge.prototype,"consumes",void 0),c([N()],ge.prototype,"produces",void 0),ge=c([j("esri.views.3d.webgl-engine.effects.ssao.SSAO")],ge);const mi=.5;function nt(t,e){e.receiveAmbientOcclusion?(t.uniforms.add(new O("ssaoTex",a=>a.ssao?.getTexture())),t.constants.add("blurSizePixelsInverse","float",1/ie),t.code.add(i`float evaluateAmbientOcclusionInverse() {
vec2 ssaoTextureSizeInverse = 1.0 / vec2(textureSize(ssaoTex, 0));
return texture(ssaoTex, gl_FragCoord.xy * blurSizePixelsInverse * ssaoTextureSizeInverse).r;
}
float evaluateAmbientOcclusion() {
return 1.0 - evaluateAmbientOcclusionInverse();
}`)):t.code.add(i`float evaluateAmbientOcclusionInverse() { return 1.0; }
float evaluateAmbientOcclusion() { return 0.0; }`)}let Ee=class extends De{constructor(e,a,o,r){super(e,"float",0,(n,l)=>n.setUniform1fv(e,o(l),r),a)}};function pi(t,e){t.uniforms.add(new Ee("shR",9,({lighting:a})=>a.sh.r),new Ee("shG",9,({lighting:a})=>a.sh.g),new Ee("shB",9,({lighting:a})=>a.sh.b)),t.code.add(i`vec3 calculateAmbientIrradiance(vec3 normal) {
vec3 ambientLight = 0.282095 * vec3(shR[0], shG[0], shB[0]);
vec4 sh1 = vec4(
0.488603 * normal.x,
0.488603 * normal.z,
0.488603 * normal.y,
1.092548 * normal.x * normal.y
);
vec4 sh2 = vec4(
1.092548 * normal.y * normal.z,
0.315392 * (3.0 * normal.z * normal.z - 1.0),
1.092548 * normal.x * normal.z,
0.546274 * (normal.x * normal.x - normal.y * normal.y)
);
vec4 lightingAmbientSH_R1 = vec4(shR[1], shR[2], shR[3], shR[4]);
vec4 lightingAmbientSH_G1 = vec4(shG[1], shG[2], shG[3], shG[4]);
vec4 lightingAmbientSH_B1 = vec4(shB[1], shB[2], shB[3], shB[4]);
ambientLight += vec3(
dot(lightingAmbientSH_R1, sh1),
dot(lightingAmbientSH_G1, sh1),
dot(lightingAmbientSH_B1, sh1)
);
vec4 lightingAmbientSH_R2 = vec4(shR[5], shR[6], shR[7], shR[8]);
vec4 lightingAmbientSH_G2 = vec4(shG[5], shG[6], shG[7], shG[8]);
vec4 lightingAmbientSH_B2 = vec4(shB[5], shB[6], shB[7], shB[8]);
ambientLight += vec3(
dot(lightingAmbientSH_R2, sh2),
dot(lightingAmbientSH_G2, sh2),
dot(lightingAmbientSH_B2, sh2)
);
return ambientLight;
}`),e.pbrMode!==1&&e.pbrMode!==2||t.code.add(i`const vec3 skyTransmittance = vec3(0.9, 0.9, 1.0);
vec3 calculateAmbientRadiance()
{
vec3 ambientLight = 1.2 * (0.282095 * vec3(shR[0], shG[0], shB[0])) - 0.2;
return ambientLight *= skyTransmittance;
}`)}function de(t){t.uniforms.add(new we("mainLightDirection",e=>e.lighting.mainLight.direction))}function ue(t){t.uniforms.add(new we("mainLightIntensity",e=>e.lighting.mainLight.intensity))}function fi(t){de(t),ue(t),t.code.add(i`vec3 applyShading(vec3 shadingNormal, float shadow) {
float dotVal = clamp(dot(shadingNormal, mainLightDirection), 0.0, 1.0);
return mainLightIntensity * ((1.0 - shadow) * dotVal);
}`)}function vi(t){t.code.add(i`vec3 evaluateDiffuseIlluminationHemisphere(vec3 ambientGround, vec3 ambientSky, float NdotNG) {
return ((1.0 - NdotNG) * ambientGround + (1.0 + NdotNG) * ambientSky) * 0.5;
}`),t.code.add(i`float integratedRadiance(float cosTheta2, float roughness) {
return (cosTheta2 - 1.0) / (cosTheta2 * (1.0 - roughness * roughness) - 1.0);
}`),t.code.add(i`vec3 evaluateSpecularIlluminationHemisphere(vec3 ambientGround, vec3 ambientSky, float RdotNG, float roughness) {
float cosTheta2 = 1.0 - RdotNG * RdotNG;
float intRadTheta = integratedRadiance(cosTheta2, roughness);
float ground = RdotNG < 0.0 ? 1.0 - intRadTheta : 1.0 + intRadTheta;
float sky = 2.0 - ground;
return (ground * ambientGround + sky * ambientSky) * 0.5;
}`)}function st(t,e){t.include(Ie),t.include(ot),e.pbrMode!==1&&e.pbrMode!==2&&e.pbrMode!==5&&e.pbrMode!==6||(t.code.add(i`float normalDistribution(float NdotH, float roughness)
{
float a = NdotH * roughness;
float b = roughness / (1.0 - NdotH * NdotH + a * a);
return b * b * INV_PI;
}`),t.code.add(i`const vec4 c0 = vec4(-1.0, -0.0275, -0.572,  0.022);
const vec4 c1 = vec4( 1.0,  0.0425,  1.040, -0.040);
const vec2 c2 = vec2(-1.04, 1.04);
vec2 prefilteredDFGAnalytical(float roughness, float NdotV) {
vec4 r = roughness * c0 + c1;
float a004 = min(r.x * r.x, exp2(-9.28 * NdotV)) * r.x + r.y;
return c2 * a004 + r.zw;
}`),t.code.add(i`struct PBRShadingInfo
{
float NdotV;
float NdotL;
float LdotH;
float NdotUP;
float RdotUP;
float NdotAmbDir;
float NdotH_Horizon;
float NdotH;
vec3 skyRadianceToSurface;
vec3 groundRadianceToSurface;
vec3 skyIrradianceToSurface;
vec3 groundIrradianceToSurface;
vec3 reflectedView;
float averageAmbientRadiance;
vec3 albedoLinear;
vec3 f0;
vec3 f90;
vec3 diffuseColor;
float metalness;
float roughness;
};`),t.code.add(i`void calculateCommonInputs(out PBRShadingInfo inputs, vec3 normal, vec3 viewDirection, vec3 upDirection, vec3 albedo) {
vec3 h = normalize(mainLightDirection - viewDirection);
inputs.NdotV = clamp(abs(dot(normal, -viewDirection)), 0.001, 1.0);
inputs.NdotUP = clamp(dot(normal, upDirection), -1.0, 1.0);
inputs.reflectedView = normalize(reflect(-viewDirection, normal));
inputs.RdotUP = clamp(dot(inputs.reflectedView, upDirection), -1.0, 1.0);
inputs.albedoLinear = linearizeGamma(albedo);
inputs.NdotH = clamp(dot(normal, h), 0.0, 1.0);
inputs.NdotL = clamp(dot(normal, mainLightDirection), 0.001, 1.0);
}`)),e.pbrMode!==1&&e.pbrMode!==2||(t.include(vi),t.code.add(i`vec3 evaluateEnvironmentIllumination(PBRShadingInfo inputs) {
vec3 indirectDiffuse = evaluateDiffuseIlluminationHemisphere(inputs.groundIrradianceToSurface, inputs.skyIrradianceToSurface, inputs.NdotUP);
vec3 indirectSpecular = evaluateSpecularIlluminationHemisphere(inputs.groundRadianceToSurface, inputs.skyRadianceToSurface, inputs.RdotUP, inputs.roughness);
vec3 diffuseComponent = inputs.diffuseColor * indirectDiffuse * INV_PI;
vec2 dfg = prefilteredDFGAnalytical(inputs.roughness, inputs.NdotV);
vec3 specularColor = inputs.f0 * dfg.x + inputs.f90 * dfg.y;
vec3 specularComponent = specularColor * indirectSpecular;
return (diffuseComponent + specularComponent);
}`),t.code.add(i`void calculatePBRInputs(out PBRShadingInfo inputs, vec3 normal, vec3 viewDirection, vec3 upDirection, vec3 albedo, vec3 mrr) {
calculateCommonInputs(inputs, normal, viewDirection, upDirection, albedo);
inputs.metalness = mrr[0];
inputs.roughness = clamp(mrr[1] * mrr[1], 0.001, 0.99);
inputs.f0 = (0.16 * mrr[2] * mrr[2]) * (1.0 - inputs.metalness) + inputs.albedoLinear * inputs.metalness;
inputs.f90 = vec3(clamp(dot(inputs.f0, vec3(50.0 * 0.33)), 0.0, 1.0));
inputs.diffuseColor = inputs.albedoLinear * (vec3(1.0) - inputs.f0) * (1.0 - inputs.metalness);
}`)),e.pbrMode!==5&&e.pbrMode!==6||t.code.add(i`const vec3 fresnelReflectionSimplified = vec3(0.04);
void calculateSimplifiedInputs(out PBRShadingInfo inputs, vec3 normal, vec3 viewDirection, vec3 upDirection, vec3 albedo) {
calculateCommonInputs(inputs, normal, viewDirection, upDirection, albedo);
float lightness = 0.3 * inputs.albedoLinear[0] + 0.5 * inputs.albedoLinear[1] + 0.2 * inputs.albedoLinear[2];
inputs.f0 = (0.85 * lightness + 0.15) * fresnelReflectionSimplified;
inputs.f90 =  vec3(clamp(dot(inputs.f0, vec3(50.0 * 0.33)), 0.0, 1.0));
}`)}function mn(t,e){t.include(ot),t.code.add(i`
    struct PBRShadingWater {
      float NdotL;   // cos angle between normal and light direction
      float NdotV;   // cos angle between normal and view direction
      float NdotH;   // cos angle between normal and half vector
      float VdotH;   // cos angle between view direction and half vector
      float LdotH;   // cos angle between light direction and half vector
      float VdotN;   // cos angle between view direction and normal vector
    };

    float dtrExponent = ${e.useCustomDTRExponentForWater?"2.2":"2.0"};
  `),t.code.add(i`vec3 fresnelReflection(float angle, vec3 f0, float f90) {
return f0 + (f90 - f0) * pow(1.0 - angle, 5.0);
}`),t.code.add(i`float normalDistributionWater(float NdotH, float roughness) {
float r2 = roughness * roughness;
float NdotH2 = NdotH * NdotH;
float denom = pow((NdotH2 * (r2 - 1.0) + 1.0), dtrExponent) * PI;
return r2 / denom;
}`),t.code.add(i`float geometricOcclusionKelemen(float LoH) {
return 0.25 / (LoH * LoH);
}`),t.code.add(i`vec3 brdfSpecularWater(in PBRShadingWater props, float roughness, vec3 F0, float F0Max) {
vec3  F = fresnelReflection(props.VdotH, F0, F0Max);
float dSun = normalDistributionWater(props.NdotH, roughness);
float V = geometricOcclusionKelemen(props.LdotH);
float diffusionSunHaze = mix(roughness + 0.045, roughness + 0.385, 1.0 - props.VdotH);
float strengthSunHaze  = 1.2;
float dSunHaze = normalDistributionWater(props.NdotH, diffusionSunHaze) * strengthSunHaze;
return ((dSun + dSunHaze) * V) * F;
}`)}function gi(t){t.include(Q),t.uniforms.add(new tt("zProjectionMapLastFrame",e=>to(e.reprojection.lastFrameCamera))),t.code.add(i`float linearDepthFromTextureLastFrame(sampler2D depthTexture, vec2 uv) {
return linearizeDepth(depthFromTexture(depthTexture, uv), zProjectionMapLastFrame);
}`)}function xi(t,e){const a=t.fragment;a.include(Q),a.uniforms.add(new tt("nearFar",o=>o.camera.nearFar),new O("depthMap",o=>o.depth?.attachment),new Me("proj",o=>o.camera.projectionMatrix),new R("invResolutionHeight",o=>1/o.camera.height),new Me("reprojectionMatrix",o=>o.reprojection.matrix)).code.add(i`
  vec2 reprojectionCoordinate(vec3 projectionCoordinate) {
    vec4 clipDepthCoordinate = proj * vec4(0.0, 0.0, -projectionCoordinate.z, 1.0);
    vec4 reprojectedCoordinate = reprojectionMatrix * vec4(
      clipDepthCoordinate.w * (projectionCoordinate.xy * 2.0 - 1.0),
      clipDepthCoordinate.z,
      clipDepthCoordinate.w
    );
    reprojectedCoordinate.xy /= reprojectedCoordinate.w;
    return reprojectedCoordinate.xy * 0.5 + 0.5;
  }

  vec4 applyProjectionMat(mat4 projectionMat, vec3 viewPosition) {
    vec4 projectedCoordinate =  projectionMat * vec4(viewPosition, 1.0);
    projectedCoordinate.xy /= projectedCoordinate.w;
    projectedCoordinate.xy = projectedCoordinate.xy*0.5 + 0.5;
    return projectedCoordinate;
  }

  float rayMarchScreenReachFromWorldReach(vec3 startPosition, vec3 rayDirection, float rayMarchWorldReach) {
    float rayDistanceWorld = max(0.0, rayMarchWorldReach);

    // Stop rays towards camera at near plane
    if (rayDirection.z > 0.0) {
      float distanceToNearPlane = (-nearFar[0] - startPosition.z) / rayDirection.z;
      rayDistanceWorld = min(rayDistanceWorld, max(0.0, distanceToNearPlane));
    }

    vec2 projectedCoordStart = applyProjectionMat(proj, startPosition).xy;
    vec2 projectedCoordEnd = applyProjectionMat(proj, startPosition + rayDirection * rayDistanceWorld).xy;
    vec2 projectedCoordOffset = projectedCoordEnd - projectedCoordStart;

    return ${e.useProjectedRayLength?"length(projectedCoordOffset)":"abs(projectedCoordOffset.y)"};
  }

  vec3 screenSpaceIntersectionWithLimits(
    vec3 rayDirection,
    vec3 startPosition,
    vec3 viewDirection,
    vec3 normal,
    float rayStepOffset,
    float rayMarchMaxReach,
    float rayMarchMaxSteps
  ) {
    vec3 viewPosition = startPosition;

    // Project the start position to the screen
    vec4 projectedCoordStart = applyProjectionMat(proj, viewPosition);
    vec3 homogeneousStart = viewPosition / projectedCoordStart.w;
    float inverseWStart = 1.0 / projectedCoordStart.w;

    // Advance the position in the ray direction
    viewPosition += rayDirection;

    vec4 projectedCoordVanishingPoint = applyProjectionMat(proj, rayDirection);

    // Project the advanced position to the screen
    vec4 projectedCoordEnd = applyProjectionMat(proj, viewPosition);
    vec3  homogeneousEnd = viewPosition / projectedCoordEnd.w;
    float inverseWEnd = 1.0 / projectedCoordEnd.w;

    // Calculate the ray direction in screen space
    vec2 projectedCoordDirection = (projectedCoordEnd.xy - projectedCoordStart.xy);
    vec2 vanishingPointScreenOffset = (projectedCoordVanishingPoint.xy - projectedCoordStart.xy);

    float rayMarchDistance = ${e.useProjectedRayLength?"length(vanishingPointScreenOffset.xy)":"abs(vanishingPointScreenOffset.y)"};
    float clampedRayMarchDistance = min(rayMarchDistance, rayMarchMaxReach);

    float projectedCoordDirectionLength = length(projectedCoordDirection);

    // normalize the projection direction depending on maximum steps
    // this determines how blocky the ray march looks
    vec2 projectedStep = clampedRayMarchDistance * projectedCoordDirection / (rayMarchMaxSteps * projectedCoordDirectionLength);

    // Normalize the homogeneous camera space coordinates
    vec3 homogeneousStep = clampedRayMarchDistance * (homogeneousEnd - homogeneousStart) / (rayMarchMaxSteps * projectedCoordDirectionLength);
    float inverseWStep = clampedRayMarchDistance * (inverseWEnd - inverseWStart) / (rayMarchMaxSteps * projectedCoordDirectionLength);

    // initialize the variables for ray marching
    vec2 projectedPosition = projectedCoordStart.xy;
    vec3 homogeneousPosition = homogeneousStart;
    float inverseW = inverseWStart;
    float rayStartZ = -startPosition.z; // estimated ray start depth value
    float rayEndZ = -startPosition.z;   // estimated ray end depth value
    float previousEstimatedZ = -startPosition.z;
    float rayDepthDelta = 0.0;
    float estimatedDepthDifference;
    float sampledDepth;

    if (dot(normal, rayDirection) < 0.0 || dot(-viewDirection, normal) < 0.0) {
      return vec3(projectedPosition, 0.0);
    }

    float previousEstimatedDepthDifference = 0.0;

    projectedPosition = clamp(
      projectedPosition + rayStepOffset * projectedStep,
      vec2(0.0),
      vec2(0.999)
    );
    homogeneousPosition.z += rayStepOffset * homogeneousStep.z;
    inverseW += rayStepOffset * inverseWStep;

    int rayMarchMaxStepsInt = int(rayMarchMaxSteps);
    for(int stepIndex = 0; stepIndex < rayMarchMaxStepsInt - 1; ++stepIndex) {
      sampledDepth = -linearDepthFromTexture(depthMap, projectedPosition); // get linear depth from the depth buffer

      // Estimate depth of the marching ray
      rayStartZ = previousEstimatedZ;
      estimatedDepthDifference = -rayStartZ - sampledDepth;
      rayEndZ = (homogeneousStep.z * 0.5 + homogeneousPosition.z) / (inverseWStep * 0.5 + inverseW);
      rayDepthDelta = rayEndZ - rayStartZ;
      previousEstimatedZ = rayEndZ;

      if(-rayEndZ > nearFar[1] || -rayEndZ < nearFar[0] || projectedPosition.y < 0.0  || projectedPosition.y > 1.0 ) {
        return vec3(projectedPosition, 0.);
      }

      // If we detect a hit - return the intersection point, two conditions:
      //  - estimatedDepthDifference > 0.0 - sampled point depth is in front of estimated depth
      //  - if difference between estimatedDepthDifference and rayDepthDelta is not too large
      //  - if difference between estimatedDepthDifference and 0.025/abs(inverseW) is not too large
      //  - if the sampled depth is not behind far plane or in front of near plane

      if(estimatedDepthDifference < 0.025 / abs(inverseW) + abs(rayDepthDelta) &&
        estimatedDepthDifference > 0.0 &&
        sampledDepth > nearFar[0] &&
        sampledDepth < nearFar[1] &&
        abs(projectedPosition.y - projectedCoordStart.y) > invResolutionHeight) {
        float hitInterpolationWeight = estimatedDepthDifference / (estimatedDepthDifference - previousEstimatedDepthDifference);
        vec2 refinedProjectedPosition = mix(projectedPosition - projectedStep, projectedPosition, 1.0 - hitInterpolationWeight);
        if (abs(refinedProjectedPosition.y - projectedCoordStart.y) > invResolutionHeight) {
          return vec3(refinedProjectedPosition, sampledDepth);
        }
        else {
          return vec3(projectedPosition, sampledDepth);
        }
      }

      ${f(!e.clampRayToScreen,`if (projectedPosition.x <= 0.0  || projectedPosition.x >= 1.0) {
        return vec3(projectedPosition, 0.0);
      }`)}

      // Continue with ray marching
      projectedPosition = projectedPosition + projectedStep;
      homogeneousPosition.z += homogeneousStep.z;
      inverseW += inverseWStep;
      previousEstimatedDepthDifference = estimatedDepthDifference;

      ${f(e.clampRayToScreen,"projectedPosition = clamp(projectedPosition, vec2(0.0), vec2(0.999));")}
    }
    return vec3(projectedPosition, 0.0);
  }

  vec3 screenSpaceIntersection(vec3 rayDirection, vec3 startPosition, vec3 viewDirection, vec3 normal, float rayStepOffset) {
    return screenSpaceIntersectionWithLimits(
      rayDirection,
      startPosition,
      viewDirection,
      normal,
      rayStepOffset,
      ${i.float(e.rayMarchMaxReach)},
      ${i.float(e.rayMarchMaxSteps)}
    );
  }
  `)}const Pt=255;function lt(t){t.code.add(i`
    vec3 quantizeGlobalIlluminationColor(vec3 color) {
      vec3 clampedColor = clamp(color, vec3(0.0), vec3(1.0));
      return floor(clampedColor * ${i.float(Pt)} + 0.5) * ${i.float(1/Pt)};
    }
  `)}const bi=.01,Ft=.008,yi=.002,wi=.5,Mi=.02,Si=.1,jt=.008,Ti=.3,_i=.008,Ci=150,Ii=.095,Di=.008,$i=60,zi=2,Pi=.0039,Fi=.25,ji=.012,Oi=0,Ri=6,ct=.15,dt=25,ut=.15,je=.5,ht=1,mt=1,Oe=16;class pt extends A{constructor(){super(...arguments),this.projScale=1,this.scaleGlobalIllumination=1,this.accumulatedFrames=0,this.temporalSampleFrame=0,this.rayMarchMinReach=ut,this.rayMarchMaxReach=je,this.rayMarchWorldReach=dt,this.rayMarchMinReachEmissionWeight=ht,this.rayMarchMaxReachEmissionWeight=mt,this.rayMarchMaxSteps=Oe,this.colorBleedWeight=ct,this.highQualityIdleColorBlendStart=Ti}}function na(t){const e=new V,a=e.fragment;e.include(me),e.include(ze),de(a),a.include(gi),a.include(Ie),a.include(lt),e.include(xi,t),a.uniforms.add(new b("normalMap",n=>n.normalTexture),new b("depthMap",n=>n.depthTexture),new O("lastFrameColorTexture",n=>n.reprojection.lastFrameColor?.getTexture()),new O("lastFrameDepthTexture",n=>n.reprojection.lastFrameDepth?.attachment),new O("lastFrameGlobalIlluminationTexture",n=>n.globalIllumination?.getTexture()),new O("lastFrameGlobalIlluminationWeightTexture",n=>n.globalIllumination?.getTexture(G)),new Me("reprojectionViewMatrix",n=>n.reprojection.viewMatrix),new Me("view",n=>n.camera.viewMatrix),new x("accumulatedFrames",n=>n.accumulatedFrames),new x("temporalSampleFrame",n=>n.temporalSampleFrame),new x("scaleGlobalIllumination",n=>n.scaleGlobalIllumination)),a.uniforms.add(new x("rayMarchMinReach",n=>n.rayMarchMinReach),new x("rayMarchMaxReach",n=>n.rayMarchMaxReach),new x("rayMarchWorldReach",n=>n.rayMarchWorldReach),new x("rayMarchMinReachEmissionWeight",n=>n.rayMarchMinReachEmissionWeight),new x("rayMarchMaxReachEmissionWeight",n=>n.rayMarchMaxReachEmissionWeight),new x("rayMarchMaxSteps",n=>n.rayMarchMaxSteps),new x("colorBleedWeight",n=>n.colorBleedWeight),new x("highQualityIdleColorBlendStart",n=>n.highQualityIdleColorBlendStart));const{hasEmission:o,hasColor:r}=t;return o&&a.uniforms.add(new R("inputScale",n=>n.reprojection.lastFrameEmission?.attachment.descriptor.internalFormat===Ga.RGBA8?Xa:1),new O("lastFrameEmissionTexture",n=>n.reprojection.lastFrameEmission?.attachment)),a.code.add(i`
    float computeIdleColorBlendWeight(float accumulatedFrames) {
      float idleColorBlendProgress = clamp(
        accumulatedFrames / ${i.float(Ci)},
        0.0,
        1.0
      );
      return mix(
        highQualityIdleColorBlendStart,
        ${i.float(_i)},
        pow(idleColorBlendProgress, 1./5.)
      );
    }

    float computeIdleOcclusionBlendWeight(float accumulatedFrames) {
      float idleOcclusionBlendProgress = clamp(
        accumulatedFrames / ${i.float($i)},
        0.0,
        1.0
      );
      return mix(
        ${i.float(Ii)},
        ${i.float(Di)},
        pow(idleOcclusionBlendProgress, ${i.float(zi)})
      );
    }

    bool isEdgeDepth(float centerDepth, vec2 sampleUv) {
      vec2 texelSize = 1.0 / vec2(textureSize(depthMap, 0));
      float depthLeft = linearizeDepth(depthFromTexture(depthMap, sampleUv + vec2(-texelSize.x, 0.0)));
      float depthRight = linearizeDepth(depthFromTexture(depthMap, sampleUv + vec2(texelSize.x, 0.0)));
      float depthUp = linearizeDepth(depthFromTexture(depthMap, sampleUv + vec2(0.0, texelSize.y)));
      float depthDown = linearizeDepth(depthFromTexture(depthMap, sampleUv + vec2(0.0, -texelSize.y)));

      float maxDifference = max(max(abs(centerDepth - depthLeft), abs(centerDepth - depthRight)), max(abs(centerDepth - depthUp), abs(centerDepth - depthDown)));

      return abs(maxDifference / centerDepth) > 0.01;
    }

    vec3 sampleCosineHemisphere(vec2 u) {
      float phi = 6.28318530718 * u.x;
      float radius = sqrt(u.y);
      float x = radius * cos(phi);
      float y = radius * sin(phi);
      float z = sqrt(max(0.0, 1.0 - u.y));

      return vec3(x, y, z);
    }

    mat3 basisFromNormal(vec3 n) {
      vec3 up = abs(n.z) < 0.999 ? vec3(0.0, 0.0, 1.0) : vec3(1.0, 0.0, 0.0);
      vec3 tangent = normalize(cross(up, n));
      vec3 bitangent = cross(n, tangent);

      return mat3(tangent, bitangent, n);
    }

    float blueNoiseDitherValue(vec2 pixel, float frame, vec2 axis, float phase) {
      float scroll = 5.588238 * mod(frame, 512.0);
      vec2 p = pixel + vec2(scroll);
      vec2 rotated = vec2(
        axis.x * p.x + axis.y * p.y,
        -axis.y * p.x + axis.x * p.y
      );

      return fract(52.9829189 * fract(0.06711056 * rotated.x + 0.00583715 * rotated.y + phase));
    }

    vec4 blueNoiseDither(vec2 pixel, float frame) {
      vec4 value = vec4(
        blueNoiseDitherValue(pixel, frame, vec2(0.9659258, 0.25881904), 0.0),
        blueNoiseDitherValue(pixel, frame, vec2(0.70710677, 0.70710677), 0.17),
        blueNoiseDitherValue(pixel, frame, vec2(0.25881904, 0.9659258), 0.37),
        blueNoiseDitherValue(pixel, frame, vec2(1.0, 0.0), 0.61)
      );

      return value * 2.0 - 1.0;
    }
  `),e.outputs.add("fragGlobalIllumination","vec4",0),e.outputs.add("fragWeight","vec2",1),a.main.add(i`
    float depth = depthFromTexture(depthMap, uv);

    // Early out if depth is out of range, such as in the sky
    if (depth >= 1.0 || depth <= 0.0) {
      fragGlobalIllumination = vec4(0.0, 0.0, 0.0, 1.0);
      fragWeight = vec2(0.0);
      return;
    }

    // Get the normal of current fragment
    ivec2 iuv = ivec2(uv * vec2(textureSize(normalMap, 0)));

    vec4 normal4 = texelFetch(normalMap, iuv, 0);

    // 0.0 alpha excludes the surface as a receiver (e.g. water)
    if (normal4.a == 0.0) {
      fragGlobalIllumination = vec4(0.0, 0.0, 0.0, 1.0);
      fragWeight = vec2(0.0);
      return;
    }

    float receiverOpacity = normal4.a;
    vec3 normal = normalize(normal4.xyz * 2.0 - 1.0);

    // Reconstruct view space position of current fragment
    float currentPixelDepth = linearizeDepth(depth);
    vec3 currentPixelPos = reconstructPosition(uv * vec2(textureSize(normalMap, 0)), currentPixelDepth);
    vec4 viewPos = vec4(currentPixelPos, 1.0);

    // Reproject current view position to last frame
    vec4 reprojectedViewPos = reprojectionViewMatrix * viewPos;
    vec4 reprojectedCoordinate = applyProjectionMat(proj, reprojectedViewPos.xyz);

    // Read last frame reprojected depth and GI history
    float lastFrameDepthViewPos = -linearDepthFromTextureLastFrame(lastFrameDepthTexture, reprojectedCoordinate.xy);
    vec4 lastFrameGlobalIllumination = texture(lastFrameGlobalIlluminationTexture, reprojectedCoordinate.xy);
    float historyOcclusionBlendWeight = texture(lastFrameGlobalIlluminationWeightTexture, reprojectedCoordinate.xy).r;

    int steps;
    float occlusionBlendWeight = 1.0;
    float colorBlendWeight = 1.0;
    float idleColorBlendWeight = computeIdleColorBlendWeight(accumulatedFrames);
    float idleOcclusionBlendWeight = computeIdleOcclusionBlendWeight(accumulatedFrames);
    float reprojectionDepthMismatch = abs((lastFrameDepthViewPos + reprojectedViewPos.z) / max(lastFrameDepthViewPos, reprojectedViewPos.z));
    bool hasReprojectionMismatch = reprojectionDepthMismatch > ${i.float(bi)};
    bool isScaledGlobalIllumination = scaleGlobalIllumination < 1.0;
    bool isLowQualityEdgePixel = isScaledGlobalIllumination && isEdgeDepth(currentPixelDepth, uv);
    bool resetColorHistory = false;

    // Heuristic to determine blending weights and number of steps for occlusion and color
    if (hasReprojectionMismatch) {
      if (isLowQualityEdgePixel) {
        steps = 1;
        occlusionBlendWeight = ${i.float(Ft)};
        resetColorHistory = true;
      } else {
        steps = 6;
        occlusionBlendWeight = 1.0;
        resetColorHistory = true;
      }
    } else {
      steps = 1;
      if (historyOcclusionBlendWeight > ${i.float(wi)}) {
        occlusionBlendWeight = ${i.float(Si)};
        colorBlendWeight = ${i.float(jt)};
      } else if (historyOcclusionBlendWeight > ${i.float(Mi)}) {
        occlusionBlendWeight = historyOcclusionBlendWeight - 0.05;
        colorBlendWeight = ${i.float(jt)};
      } else {
        occlusionBlendWeight = isScaledGlobalIllumination ? ${i.float(Ft)} : idleOcclusionBlendWeight;
        colorBlendWeight = isScaledGlobalIllumination ? ${i.float(yi)} : idleColorBlendWeight;
      }
    }

    vec4 randomDirectionSample;
    mat3 normalBasis = basisFromNormal(normal);
    int temporalSampleStride = min(64 / steps, 6);
    float temporalFrameOffset = mod(temporalSampleFrame, float(64 / steps));

    // For each ray determine if it hits geometry and accumulate occlusion or color
    float stepSize = 1.0 / float(steps);
    ${f(r,i`
      float colorBleedScale = mix(
        1.0,
        ${i.float(Oi)},
        smoothstep(
          ${i.float(ji)},
          highQualityIdleColorBlendStart,
          colorBlendWeight
        )
      );
      `)}
    for (int i = 0; i < steps; ++i) {
      float sampleIndex = float(i * temporalSampleStride + int(temporalFrameOffset));
      randomDirectionSample = blueNoiseDither(floor(gl_FragCoord.xy), sampleIndex);
      vec2 hemisphereSample = randomDirectionSample.rg * 0.5 + 0.5;
      float offsetSample = randomDirectionSample.a * 0.5 + 0.5;
      vec3 rayDirection = normalBasis * sampleCosineHemisphere(hemisphereSample);
      float rayMarchScreenReach = rayMarchScreenReachFromWorldReach(viewPos.xyz, rayDirection, rayMarchWorldReach);
      rayMarchScreenReach = clamp(rayMarchScreenReach, rayMarchMinReach, rayMarchMaxReach);
      vec3 hit = screenSpaceIntersectionWithLimits(
        rayDirection,
        viewPos.xyz,
        normalize(viewPos.xyz),
        normal,
        offsetSample,
        rayMarchScreenReach,
        rayMarchMaxSteps
      );

      float hitOpacity = 0.0;
      if (hit.z > 0.0) {
        ivec2 normalMapSize = textureSize(normalMap, 0);
        ivec2 hitIuv = clamp(ivec2(hit.xy * vec2(normalMapSize)), ivec2(0), normalMapSize - ivec2(1));
        float storedHitOpacity = texelFetch(normalMap, hitIuv, 0).a;

        // 0.0 alpha excludes the surface as a receiver (e.g. water), but a valid ray hit still occludes as opaque
        hitOpacity = storedHitOpacity == 0.0 ? 1.0 : storedHitOpacity;

        ${f(r,i`
          // Emission and color bleed - Reproject the current receiver and sampled hit to estimate bounced color
          vec3 receiverColor = texture(lastFrameColorTexture, reprojectedCoordinate.xy).rgb;

          vec2 hitReprojectedCoordinate = reprojectionCoordinate(hit);
          vec3 sourceColor = texture(lastFrameColorTexture, hitReprojectedCoordinate).rgb;
          vec3 sourceColorLinear = linearizeGamma(sourceColor);
          vec3 sourceEmission = ${f(o,"texture(lastFrameEmissionTexture, hitReprojectedCoordinate).xyz * inputScale","vec3(0.0)")};

          float emissionWeight = mix(
            rayMarchMinReachEmissionWeight,
            rayMarchMaxReachEmissionWeight,
            (rayMarchScreenReach - rayMarchMinReach) / max(rayMarchMaxReach - rayMarchMinReach, 0.00001)
          );
          fragGlobalIllumination.rgb += hitOpacity * (
            (sourceColorLinear * colorBleedWeight * colorBleedScale) + sourceEmission * emissionWeight
          ) * stepSize;
          `)}
      }

      if (hitOpacity < 1.0) {
        // Occlusion - heuristic modulating sky intensity based on angle to main light
        vec4 viewMainLightDirection = view * vec4(mainLightDirection, 0.0);
        float skyModulation = pow(max(dot(rayDirection, viewMainLightDirection.xyz), 0.0), 3.0) * 5.5;
        float skyFacingWeight = clamp(3.5 * dot(viewMainLightDirection.xyz, normal), 0.0, 1.0);
        skyModulation = mix(1.0, skyModulation * 0.2 + 0.8, skyFacingWeight);
        fragGlobalIllumination.a += (1.0 - hitOpacity) * skyModulation * stepSize;
      }
    }

    fragGlobalIllumination.rgb *= receiverOpacity;
    fragGlobalIllumination.a = mix(1.0, fragGlobalIllumination.a, receiverOpacity);

    // Rendering trick add noise to reduce accumulation artifacts
    float accumulationDither = occlusionBlendWeight < 1.0
      ? receiverOpacity * randomDirectionSample.b * ${i.float(Pi)}
      : 0.0;

    ${f(r,i`
      // Accumulate color
      vec3 lastFrameColor = lastFrameGlobalIllumination.rgb;
      float colorDitherScale = isScaledGlobalIllumination ? ${i.float(Fi)} : 1.0;
      fragGlobalIllumination.rgb = resetColorHistory
        ? vec3(0.0)
        : mix(lastFrameColor + accumulationDither * colorDitherScale, fragGlobalIllumination.rgb, colorBlendWeight);
      `,i`
      fragGlobalIllumination.rgb = vec3(0.0);
      `)}
    fragGlobalIllumination.rgb = quantizeGlobalIlluminationColor(fragGlobalIllumination.rgb);

    // Accumulate occlusion
    fragGlobalIllumination.a = mix(lastFrameGlobalIllumination.a + accumulationDither, fragGlobalIllumination.a, occlusionBlendWeight);

    fragWeight = vec2(occlusionBlendWeight, colorBlendWeight * ${i.float(Ri)});
  `),e}const Wi=Object.freeze(Object.defineProperty({__proto__:null,GlobalIlluminationPassParameters:pt,build:na,defaultColorBleedWeight:ct,defaultRayMarchMaxReach:je,defaultRayMarchMaxReachEmissionWeight:mt,defaultRayMarchMaxSteps:Oe,defaultRayMarchMinReach:ut,defaultRayMarchMinReachEmissionWeight:ht,defaultRayMarchWorldReach:dt},Symbol.toStringTag,{value:"Module"})),Bi=15.3,Gi=5,Ni=-.05;function sa(t){t.fragment.code.add(i`
    float globalIlluminationNormalSimilarityWeight(vec3 sampleNormal, vec3 centerNormal) {
      return clamp(1.0 - ${i.float(Bi)} * length(sampleNormal - centerNormal), 0.0, 1.0);
    }

    float globalIlluminationDepthNormalCorrection(vec3 encodedNormal) {
      vec3 decodedNormal = normalize(encodedNormal * 2.0 - 1.0);
      return pow(max((1.0 - abs(decodedNormal.x)) * (1.0 - abs(decodedNormal.y)), 0.01), ${i.float(Gi)});
    }

    float globalIlluminationDepthSharpness(float projScale, float depth) {
      return ${i.float(Ni)} * projScale / depth;
    }

    float globalIlluminationDepthSharpness(float projScale, float depth, vec3 encodedNormal) {
      return globalIlluminationDepthSharpness(projScale, depth) * globalIlluminationDepthNormalCorrection(encodedNormal);
    }
  `)}const Le=4,Ei=.2,Li=2.5;let ft=class extends A{constructor(){super(...arguments),this.blurSize=Ke()}};function la(){const t=new V,e=t.fragment;t.include(me),t.include(ze),t.include(sa);const a=(Le+3)/2,o=1/(2*a*a);e.include(Q),e.include(co,ca),e.include(lt);const r=5e4;e.uniforms.add(new Qe("hasEmission",v=>v.reprojection.lastFrameEmission!=null),new b("depthMap",v=>v.depthTexture),new b("normalMap",v=>v.normalTexture),new W("globalIlluminationTexture",v=>v.texture),new W("globalIlluminationWeightTexture",v=>v.weightTexture),new at("blurSize",v=>v.blurSize),new x("scaleGlobalIllumination",v=>v.scaleGlobalIllumination),new x("projScale",(v,C)=>{const y=C.camera.distance;return y>r?Math.max(0,v.projScale-(y-r)):v.projScale}));const n=.1,l=400,s=.03,d=.08,u=1.5,g=1/32768;return e.code.add(i`
    void accumulateBlurSample(
      vec2 sampleUv,
      float sampleOffset,
      float centerDepth,
      vec3 centerNormal,
      float depthSharpness,
      bool skipOcclusionBlur,
      inout float emissionWeightSum,
      inout vec3 emissionSum,
      inout float occlusionWeightSum,
      inout float occlusionSum,
      float centerOcclusionBlendWeight
    ) {
      vec4 sampleGlobalIllumination = texture(globalIlluminationTexture, sampleUv);
      vec3 sampleNormal = texture(normalMap, sampleUv).rgb;
      float sampleDepth = linearDepthFromTexture(depthMap, sampleUv);

      float depthDelta = sampleDepth - centerDepth;
      bool isScaledGlobalIllumination = scaleGlobalIllumination < 1.0;
      float normalSimilarityWeight = globalIlluminationNormalSimilarityWeight(sampleNormal, centerNormal);
      float depthNormalCorrection = globalIlluminationDepthNormalCorrection(sampleNormal);
      vec3 emission = sampleGlobalIllumination.rgb;
      float emissionSpatialWeightMultiplier = isScaledGlobalIllumination ? ${i.float(l)} : 1.0;

      float emissionWeight = exp(
        -sampleOffset * sampleOffset * ${i.float(o)} * ${i.float(n)} * emissionSpatialWeightMultiplier
        - depthDelta * depthDelta * depthSharpness * depthNormalCorrection
      );
      emissionWeight *= normalSimilarityWeight;
      emissionWeightSum += emissionWeight;
      emissionSum += emissionWeight * emission;

      if (skipOcclusionBlur) {
        return;
      }

      float occlusionSpatialKernelScale = centerOcclusionBlendWeight > ${i.float(s)}
        ? ${i.float(d)}
        : ${i.float(u)};
      float occlusionWeight = exp(-sampleOffset * sampleOffset * occlusionSpatialKernelScale - depthDelta * depthDelta * depthSharpness);
      occlusionWeight *= normalSimilarityWeight;
      occlusionWeightSum += occlusionWeight;
      occlusionSum += occlusionWeight * sampleGlobalIllumination.a;
    }
  `),e.main.add(i`
    vec3 emissionSum = vec3(0.0);
    float emissionWeightSum = 0.0;

    vec4 centerGlobalIllumination = texture(globalIlluminationTexture, uv);
    float centerOcclusionBlendWeight = texture(globalIlluminationWeightTexture, uv).r;
    float centerColorBlendWeight = texture(globalIlluminationWeightTexture, uv).g;
    bool isScaledGlobalIllumination = scaleGlobalIllumination < 1.0;
    bool shouldReuseCenterOcclusion = isScaledGlobalIllumination && centerOcclusionBlendWeight <= ${i.float(s)};
    bool shouldSkipLowQualityBlur = !hasEmission && shouldReuseCenterOcclusion;
    if (shouldSkipLowQualityBlur) {
      fragColor = vec4(
        quantizeGlobalIlluminationColor(centerGlobalIllumination.rgb),
        centerGlobalIllumination.a
      );
      return;
    }

    float centerDepth = linearDepthFromTexture(depthMap, uv);
    vec3 centerNormal = texture(normalMap, uv).rgb;
    float occlusionSum = 0.0;
    float occlusionWeightSum = 0.0;

    float depthSharpness = globalIlluminationDepthSharpness(projScale, centerDepth);
    float lowQualityBlurScale = hasEmission
      ? mix(
          1.0,
          ${i.float(Li)},
          smoothstep(0.0, ${i.float(Ei)}, centerColorBlendWeight)
        )
      : 1.0;
    for (int sampleOffset = -${i.int(Le)}; sampleOffset <= ${i.int(Le)}; ++sampleOffset) {
      float sampleOffsetFloat = float(sampleOffset);
      vec2 sampleUv = uv + sampleOffsetFloat * blurSize * lowQualityBlurScale;
      accumulateBlurSample(
        sampleUv,
        sampleOffsetFloat,
        centerDepth,
        centerNormal,
        depthSharpness,
        shouldReuseCenterOcclusion,
        emissionWeightSum,
        emissionSum,
        occlusionWeightSum,
        occlusionSum,
        centerOcclusionBlendWeight
      );
    }

    float occlusion = shouldReuseCenterOcclusion ? centerGlobalIllumination.a : occlusionSum / occlusionWeightSum;
    vec3 blurredEmission = (emissionSum / emissionWeightSum).rgb;

    // Dither the blurred color to reduce quantization banding and wrong color accumulation.
    float dither = ditherNoise(vec4(blurredEmission, occlusion)) - ${i.float(g)};
    blurredEmission += isScaledGlobalIllumination ? ${i.float(.85)} * dither : dither;

    fragColor = vec4(quantizeGlobalIlluminationColor(blurredEmission), occlusion);
  `),t}const ca=new Ka;ca.useFloatBlend=!1;const Vi=Object.freeze(Object.defineProperty({__proto__:null,GlobalIlluminationBlurDrawParameters:ft,build:la},Symbol.toStringTag,{value:"Module"}));let ke=class extends ee{constructor(){super(...arguments),this.shader=new L(Vi,()=>E(()=>Promise.resolve().then(()=>Ir),void 0))}initializePipeline(){return te({colorWrite:ae})}};ke=c([j("esri.views.3d.webgl-engine.effects.globalIllumination.GlobalIlluminationBlurTechnique")],ke);let qe=class extends ee{constructor(){super(...arguments),this.shader=new L(Wi,()=>E(()=>Promise.resolve().then(()=>Dr),void 0))}initializePipeline(){return te({colorWrite:ae})}};qe=c([j("esri.views.3d.webgl-engine.effects.globalIllumination.GlobalIlluminationTechnique")],qe);let Ze=class extends Vt{constructor(){super(...arguments),this.hasColor=!0,this.hasEmission=!1,this.rayMarchMaxReach=je,this.rayMarchMaxSteps=Oe,this.useProjectedRayLength=!0,this.clampRayToScreen=!1}};c([m()],Ze.prototype,"hasColor",void 0),c([m()],Ze.prototype,"hasEmission",void 0);const Ot=5e4,Ai=100;let vt=class extends A{};function da(){const t=new V,e=t.fragment;return t.include(me),t.include(ze),t.include(sa),e.include(Q),e.include(lt),e.uniforms.add(new b("depthMap",a=>a.depthTexture),new b("normalMap",a=>a.normalTexture),new W("tex",a=>a.colorTexture),new W("globalIlluminationWeightTexture",a=>a.weightTexture),new x("projScale",(a,o)=>{const r=o.camera.distance;return r>Ot?Math.max(0,a.projScale-(r-Ot)):a.projScale})),e.code.add(i`
    float computeDepthWeight(float sampleDepth, float centerDepth, float depthSharpness) {
      float depthDelta = abs(sampleDepth - centerDepth);
      return exp(-0.08 - depthDelta * depthDelta * depthSharpness);
    }

    vec3 normalFromTexture(sampler2D normalTexture, vec2 uv) {
      ivec2 normalTextureSize = textureSize(normalTexture, 0);
      ivec2 normalTexel = clamp(ivec2(uv * vec2(normalTextureSize)), ivec2(0), normalTextureSize - ivec2(1));
      return texelFetch(normalTexture, normalTexel, 0).xyz;
    }

    void sampleJointBilateralUpscale(vec2 sampleUv, out vec4 upscaledColor, out vec2 upscaledWeight) {
      float centerDepth = linearDepthFromTexture(depthMap, sampleUv);
      vec3 centerNormal = normalFromTexture(normalMap, sampleUv);
      float depthSharpness = ${i.float(Ai)} * globalIlluminationDepthSharpness(projScale, centerDepth, centerNormal);

      vec2 lowResTextureSize = vec2(textureSize(tex, 0));
      vec2 texelPosition = sampleUv * lowResTextureSize - 0.5;
      vec2 texelBase = floor(texelPosition);
      vec2 bilinearWeightsFraction = fract(texelPosition);

      vec2 uv00 = (texelBase + vec2(0.5, 0.5)) / lowResTextureSize;
      vec2 uv10 = (texelBase + vec2(1.5, 0.5)) / lowResTextureSize;
      vec2 uv01 = (texelBase + vec2(0.5, 1.5)) / lowResTextureSize;
      vec2 uv11 = (texelBase + vec2(1.5, 1.5)) / lowResTextureSize;

      vec4 color00 = texture(tex, uv00);
      vec4 color10 = texture(tex, uv10);
      vec4 color01 = texture(tex, uv01);
      vec4 color11 = texture(tex, uv11);
      vec2 weight00 = texture(globalIlluminationWeightTexture, uv00).rg;
      vec2 weight10 = texture(globalIlluminationWeightTexture, uv10).rg;
      vec2 weight01 = texture(globalIlluminationWeightTexture, uv01).rg;
      vec2 weight11 = texture(globalIlluminationWeightTexture, uv11).rg;

      float depth00 = linearDepthFromTexture(depthMap, uv00);
      float depth10 = linearDepthFromTexture(depthMap, uv10);
      float depth01 = linearDepthFromTexture(depthMap, uv01);
      float depth11 = linearDepthFromTexture(depthMap, uv11);

      vec3 normal00 = normalFromTexture(normalMap, uv00);
      vec3 normal10 = normalFromTexture(normalMap, uv10);
      vec3 normal01 = normalFromTexture(normalMap, uv01);
      vec3 normal11 = normalFromTexture(normalMap, uv11);

      float bilinearWeight00 = (1.0 - bilinearWeightsFraction.x) * (1.0 - bilinearWeightsFraction.y);
      float bilinearWeight10 = bilinearWeightsFraction.x * (1.0 - bilinearWeightsFraction.y);
      float bilinearWeight01 = (1.0 - bilinearWeightsFraction.x) * bilinearWeightsFraction.y;
      float bilinearWeight11 = bilinearWeightsFraction.x * bilinearWeightsFraction.y;

      float jointBilateralWeight00 = bilinearWeight00 * computeDepthWeight(depth00, centerDepth, depthSharpness) * globalIlluminationNormalSimilarityWeight(normal00, centerNormal);
      float jointBilateralWeight10 = bilinearWeight10 * computeDepthWeight(depth10, centerDepth, depthSharpness) * globalIlluminationNormalSimilarityWeight(normal10, centerNormal);
      float jointBilateralWeight01 = bilinearWeight01 * computeDepthWeight(depth01, centerDepth, depthSharpness) * globalIlluminationNormalSimilarityWeight(normal01, centerNormal);
      float jointBilateralWeight11 = bilinearWeight11 * computeDepthWeight(depth11, centerDepth, depthSharpness) * globalIlluminationNormalSimilarityWeight(normal11, centerNormal);
      float jointBilateralWeightSum = jointBilateralWeight00 + jointBilateralWeight10 + jointBilateralWeight01 + jointBilateralWeight11;

      if (jointBilateralWeightSum < 0.0001) {
        // Fall back to the nearest low-resolution texel when all bilateral weights collapse.
        vec2 nearestUv = (floor(texelPosition + 0.5) + vec2(0.5)) / lowResTextureSize;
        upscaledColor = texture(tex, nearestUv);
        upscaledWeight = texture(globalIlluminationWeightTexture, nearestUv).rg;
        return;
      }

      upscaledColor = (
        color00 * jointBilateralWeight00 +
        color10 * jointBilateralWeight10 +
        color01 * jointBilateralWeight01 +
        color11 * jointBilateralWeight11
      ) / jointBilateralWeightSum;

      upscaledWeight = (
        weight00 * jointBilateralWeight00 +
        weight10 * jointBilateralWeight10 +
        weight01 * jointBilateralWeight01 +
        weight11 * jointBilateralWeight11
      ) / jointBilateralWeightSum;
    }
  `),t.outputs.add("fragColor","vec4",0),t.outputs.add("fragWeight","vec2",1),e.main.add(i`sampleJointBilateralUpscale(uv, fragColor, fragWeight);
fragColor.rgb = quantizeGlobalIlluminationColor(fragColor.rgb);`),t}const Hi=Object.freeze(Object.defineProperty({__proto__:null,GlobalIlluminationUpscaleDrawParameters:vt,build:da},Symbol.toStringTag,{value:"Module"}));let Je=class extends ee{constructor(){super(...arguments),this.shader=new L(Hi,()=>E(()=>Promise.resolve().then(()=>$r),void 0))}initializePipeline(){return te({colorWrite:ae})}};Je=c([j("esri.views.3d.webgl-engine.effects.globalIllumination.GlobalIlluminationUpscaleTechnique")],Je);const re=1,Ui=100;let xe=class extends Xt{constructor(t){super(t),this.consumes={required:["normals"]},this.produces=J.AMBIENT_ILLUMINATION,this._passParameters=new pt,this._drawParameters=new ft,this._drawParametersUpscale=new vt,this._maxFrames=256,this._lowQualityResolutionScale=.25,this._configuration=new Ze,this._lastOutput=null,this._isGlobalIlluminationUpdate=!1,this._resetBuffer=!1}initialize(){this.addHandles(Nt(()=>this.view.stage.renderer.hasGlobalIllumination,()=>{this._reset(),this._requestRender()},Na))}destroy(){this._lastOutput=We(this._lastOutput)}resetAccumulatedFrames(){this._isGlobalIlluminationUpdate||!(this._passParameters.accumulatedFrames>Ui)&&this._configuration.hasEmission||this._reset()}render(t){if(this._passParameters.accumulatedFrames>=this._maxFrames)return this._lastOutput?.retain(),this._lastOutput;const e=t.find(({name:Sa})=>Sa==="normals"),a=e?.getTexture(),o=e?.getTexture(Et),r=this._mode;if(!a||!o)return this._emptyOutput;if(r===0)return this._resetBuffer=!1,this._emptyOutput;if(!this._canRender)return this._resetBuffer=!1,this._requestRender(),this._emptyOutput;const n=this.bindParameters;this._configuration.hasEmission=!!n.reprojection.lastFrameEmission;const l=this.techniques.getCompiled(qe,this._configuration),s=this.techniques.getCompiled(ke),d=r===1,u=d?this._lowQualityResolutionScale:1,g=d?this.techniques.getCompiled(Je):null;if(!l||!s||d&&!g)return this._requestRender(),this._emptyOutput;const h=this.renderingContext,{camera:v}=n;this._passParameters.normalTexture=a,this._passParameters.depthTexture=o,this._passParameters.projScale=1/v.computeScreenPixelSizeAtDist(1),this._passParameters.scaleGlobalIllumination=u;const{fullWidth:C,fullHeight:y}=v,I=Math.max(1,Math.floor(C*u)),S=Math.max(1,Math.floor(y*u)),P=this.fboCache.acquire(I,S,"global illumination input").acquireColor(G,2);h.bindFramebuffer(P.fbo),h.setViewport(0,0,I,S),h.bindTechnique(l,n,this._passParameters,this._drawParameters),h.screen.draw();const $=P.obtainAttachment(G),T=Math.max(1,Math.round(I/re)),_=Math.max(1,Math.round(S/re)),D=this.fboCache.acquire(T,_,"global illumination blur horizontal");h.bindFramebuffer(D.fbo),this._drawParameters.texture=P.getTexture(),this._drawParameters.weightTexture=$.attachment,X(this._drawParameters.blurSize,0,re/S),h.bindTechnique(s,n,this._passParameters,this._drawParameters),h.setViewport(0,0,T,_),h.screen.draw(),P.release();const Re=d?"global illumination blur vertical":J.AMBIENT_ILLUMINATION,H=this.fboCache.acquire(T,_,Re);h.bindFramebuffer(H.fbo),h.setViewport(0,0,T,_),h.setClearColor(1,1,1,0),h.clear(16384),this._drawParameters.texture=D.getTexture(),this._drawParameters.weightTexture=$.attachment,X(this._drawParameters.blurSize,re/T,0),h.bindTechnique(s,n,this._passParameters,this._drawParameters),h.setViewport(0,0,T,_),h.screen.draw(),D.release(),H.attachColor($,G),$.release();let pe=H;return g&&(pe=this.fboCache.acquire(C,y,J.AMBIENT_ILLUMINATION).acquireColor(G,2),h.bindFramebuffer(pe.fbo),h.setViewport(0,0,C,y),h.setClearColor(1,1,1,0),h.clear(16384),this._drawParametersUpscale.colorTexture=H.getTexture(),this._drawParametersUpscale.weightTexture=H.getTexture(G),h.bindTechnique(g,n,this._passParameters,this._drawParametersUpscale),h.screen.draw(),H.release()),h.setViewport4fv(v.fullViewport),this._passParameters.temporalSampleFrame=(this._passParameters.temporalSampleFrame+1)%64,++this._passParameters.accumulatedFrames,this._storeOutput(pe),this._passParameters.accumulatedFrames<this._maxFrames&&this._requestRender(),pe}_requestRender(){this._isGlobalIlluminationUpdate=!0,this.requestRender(1),this._isGlobalIlluminationUpdate=!1}_storeOutput(t){this._lastOutput!==t&&(this._lastOutput=We(this._lastOutput),this._lastOutput=t,this._lastOutput.retain())}get _emptyOutput(){const t=this.renderingContext,{fullWidth:e,fullHeight:a}=this.bindParameters.camera,o=this.fboCache.acquire(e,a,J.AMBIENT_ILLUMINATION).acquireColor(G,2);return t.bindFramebuffer(o.fbo),t.setViewport(0,0,e,a),t.clearBuffer(0,[0,0,0,1]),t.clearBuffer(1,[0,0,0,0]),o}get _canRender(){const{reprojection:t,hasEmission:e,globalIllumination:a}=this.bindParameters;return!(!t.lastFrameColor||e&&!t.lastFrameEmission||!t.lastFrameDepth||!a||this._resetBuffer)}get _mode(){const{hasGlobalIlluminationHighQuality:t,hasGlobalIllumination:e}=this.view.stage.renderer;return t?2:e?1:0}_reset(){this._passParameters.accumulatedFrames=0,this._lastOutput=We(this._lastOutput)}get test(){const t=this;return{passParameters:this._passParameters,configuration:this._configuration,get maxFrames(){return t._maxFrames},set maxFrames(e){t._maxFrames=e},get lowQualityResolutionScale(){return t._lowQualityResolutionScale},set lowQualityResolutionScale(e){t._lowQualityResolutionScale=e},get mode(){return t._mode},restartAccumulation:()=>{this._reset(),this._passParameters.temporalSampleFrame=0,this._resetBuffer=!0,this._requestRender()}}}};c([N()],xe.prototype,"consumes",void 0),c([N()],xe.prototype,"produces",void 0),xe=c([j("esri.views.3d.webgl-engine.effects.globalIllumination.GlobalIllumination")],xe);function ki(t,e){e.receiveGlobalIllumination?(t.uniforms.add(new Qe("hasGlobalIlluminationTexture",a=>a.globalIllumination!=null),new O("globalIlluminationTexture",a=>a.globalIllumination?.getTexture())),t.constants.add("blurSizePixelsInverse","float",1/re),t.code.add(i`vec3 readGlobalIlluminationOcclusionInverse() {
if (!hasGlobalIlluminationTexture) {
return vec3(1.0);
}
ivec2 texel = ivec2(gl_FragCoord.xy * blurSizePixelsInverse);
return vec3(texelFetch(globalIlluminationTexture, texel, 0).a);
}
vec3 readGlobalIlluminationOcclusion() {
return 1.0 - readGlobalIlluminationOcclusionInverse();
}
vec4 readGlobalIlluminationEmissionInverse() {
if (!hasGlobalIlluminationTexture) {
return vec4(1.0);
}
ivec2 texel = ivec2(gl_FragCoord.xy * blurSizePixelsInverse);
return 1.0 - vec4(texelFetch(globalIlluminationTexture, texel, 0).rgb, 0.0);
}
vec4 readGlobalIlluminationEmission() {
return max((1.0 - readGlobalIlluminationEmissionInverse() - 0.01) / 0.99, 0.0);
}`)):t.code.add(i`vec3 readGlobalIlluminationOcclusionInverse() { return vec3(1.0); }
vec3 readGlobalIlluminationOcclusion() { return vec3(0.0); }
vec4 readGlobalIlluminationEmissionInverse() { return vec4(1.0); }
vec4 readGlobalIlluminationEmission() { return vec4(0.0); }`)}function qi(t){t.code.add(i`float mapChannel(float x, vec2 p) {
if((x < p.x) && (p.x == 0.0) || !(x < p.x) && (p.x == 1.0)) {
return 0.0;
}
float result = (x < p.x) ? mix(0.0, p.y, x/p.x) : mix(p.y, 1.0, (x - p.x) / (1.0 - p.x) );
return max(result, 0.0);
}`),t.code.add(i`vec3 blackLevelSoftCompression(vec3 color, float averageAmbientRadiance) {
vec2 p = vec2(0.02, 0.0075) * averageAmbientRadiance;
return vec3(mapChannel(color.x, p), mapChannel(color.y, p), mapChannel(color.z, p));
}`)}function Zi(t){t.code.add(i`vec3 tonemapACES(vec3 x) {
return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}`),t.code.add(i`vec3 tonemapKhronosNeutral(vec3 color) {
const float startCompression = 0.76;
const float desaturation = 0.15;
float peak = max(color.r, max(color.g, color.b));
if (peak < startCompression) {
return color;
}
float d = 1.0 - startCompression;
float newPeak = 1.0 - d * d / (peak + d - startCompression);
color *= newPeak / peak;
float g = 1.0 - 1.0 / (desaturation * (peak - newPeak) + 1.0 );
return mix(color, vec3(newPeak), g);
}`)}function gt(t){t.constants.add("ambientBoostFactor","float",Fo)}function xt(t){t.uniforms.add(new R("lightingGlobalFactor",e=>e.lighting.globalFactor))}function ua(t,e){const{pbrMode:a,spherical:o,hasColorTexture:r,receiveGlobalIllumination:n}=e;t.include(Ie),t.include(ki,e),t.include(nt,e),a!==0&&t.include(st,e),t.include(pi,e),t.include(ot),t.include(Zi,e);const l=!(a===2&&!r);l&&t.include(qi),gt(t),xt(t),de(t),t.code.add(i`
    float additionalDirectedAmbientLight(float lightAlignment) {
      return smoothstep(0.0, 1.0, clamp(lightAlignment * 2.5, 0.0, 1.0));
    }

    float additionalDirectedAmbientLight(vec3 vPosWorld) {
      float lightAlignment = dot(${o?i`normalize(vPosWorld)`:i`vec3(0.0, 0.0, 1.0)`}, mainLightDirection);
      return smoothstep(0.0, 1.0, clamp(lightAlignment * 2.5, 0.0, 1.0));
    }
  `),ue(t),t.code.add(i`vec3 evaluateAdditionalLighting(float ambientOcclusion, vec3 vPosWorld) {
float additionalAmbientScale = additionalDirectedAmbientLight(vPosWorld);
return (1.0 - ambientOcclusion) * additionalAmbientScale * ambientBoostFactor * lightingGlobalFactor * mainLightIntensity;
}`);const s=n?"globalIlluminationOcclusion":"ssao",d=n?.75:1,u=n?1.5:1;switch(a){case 0:case 4:case 3:t.include(fi),t.code.add(i`vec3 evaluateSceneLighting(vec3 normalWorld, vec3 albedo, float shadow, float ssao, vec3 additionalLight) {
vec3 mainLighting = applyShading(normalWorld, shadow);
vec3 ambientLighting = calculateAmbientIrradiance(normalWorld) * (1.0 - ssao);
vec3 albedoLinear = linearizeGamma(albedo);
vec3 totalLight = mainLighting + ambientLighting + additionalLight;
totalLight = min(totalLight, vec3(PI));
vec3 outColor = vec3((albedoLinear / PI) * totalLight);
return delinearizeGamma(outColor);
}`);break;case 1:case 2:{const g=n?.35:.2;t.code.add(i`
        const float fillLightIntensity = 0.25;
        const float horizonLightDiffusion = 0.4;
        const float additionalAmbientIrradianceFactor = 0.02;
        const float groundReflectance = ${i.float(g)};

        vec3 evaluateSceneLightingPBR(vec3 normal, vec3 albedo, float shadow, float ssao, vec3 additionalLight,
                                      vec3 viewDirection, vec3 upDirection, vec3 mrr, float additionalAmbientIrradiance) {
          PBRShadingInfo inputs;
          calculatePBRInputs(inputs, normal, viewDirection, upDirection, albedo, mrr);

          ${f(n,i`vec3 globalIlluminationOcclusion = min(1.2 * readGlobalIlluminationOcclusion(), 1.0);`)}
      `),e.useFillLights?t.uniforms.add(new Qe("hasFillLights",h=>h.enableFillLights)):t.constants.add("hasFillLights","bool",!1),t.code.add(i`
        vec3 ambientDir = vec3(5.0 * upDirection[1] - upDirection[0] * upDirection[2], - 5.0 * upDirection[0] - upDirection[2] * upDirection[1], upDirection[1] * upDirection[1] + upDirection[0] * upDirection[0]);
        ambientDir = ambientDir != vec3(0.0) ? normalize(ambientDir) : normalize(vec3(5.0, -1.0, 0.0));

        inputs.NdotAmbDir = hasFillLights ? abs(dot(normal, ambientDir)) : 1.0;

        // Calculate the irradiance components: sun, fill lights and the sky.
        vec3 mainLightIrradianceComponent = ${i.float(d)} * inputs.NdotL * (1.0 - shadow) * mainLightIntensity;
        vec3 fillLightsIrradianceComponent = inputs.NdotAmbDir * mainLightIntensity * fillLightIntensity;
        // calculate ambient irradiance for localView and additionalLight for globalView
        vec3 ambientLightIrradianceComponent = ${i.float(u)} * calculateAmbientIrradiance(normal) * (1.0 - ${s}) + additionalLight;

        // Assemble the overall irradiance of the sky that illuminates the surface
        inputs.skyIrradianceToSurface = ambientLightIrradianceComponent + mainLightIrradianceComponent + fillLightsIrradianceComponent ;
        // Assemble the overall irradiance of the ground that illuminates the surface. for this we use the simple model that changes only the sky irradiance by the groundReflectance
        inputs.groundIrradianceToSurface = groundReflectance * ambientLightIrradianceComponent + mainLightIrradianceComponent + fillLightsIrradianceComponent ;
      `),t.uniforms.add(new R("lightingSpecularStrength",h=>h.lighting.mainLight.specularStrength),new R("lightingEnvironmentStrength",h=>h.lighting.mainLight.environmentStrength)).code.add(i`
        vec3 horizonRingDir = inputs.RdotUP * upDirection - inputs.reflectedView;
        vec3 horizonRingH = normalize(horizonRingDir - viewDirection);
        inputs.NdotH_Horizon = dot(normal, horizonRingH);

        vec3 mainLightRadianceComponent = lightingSpecularStrength * normalDistribution(inputs.NdotH, inputs.roughness) * mainLightIntensity * (1.0 - shadow);
        vec3 horizonLightRadianceComponent = lightingEnvironmentStrength * normalDistribution(inputs.NdotH_Horizon, min(inputs.roughness + horizonLightDiffusion, 1.0)) * mainLightIntensity * fillLightIntensity;

        // calculateAmbientRadiance for localView and additionalLight for global view
        vec3 ambientLightRadianceComponent = lightingEnvironmentStrength * calculateAmbientRadiance() * (1.0 - ${s}) + additionalLight;
        float normalDirectionModifier = mix(1., min(mix(0.1, 2.0, (inputs.NdotUP + 1.) * 0.5), 1.0), clamp(inputs.roughness * 5.0, 0.0 , 1.0));

        // Assemble the overall radiance of the sky that illuminates the surface
        inputs.skyRadianceToSurface = (ambientLightRadianceComponent + horizonLightRadianceComponent) * normalDirectionModifier + mainLightRadianceComponent;

        // Assemble the overall radiance of the ground that illuminates the surface. for this we use the simple model that changes only the sky radiance by the groundReflectance
        inputs.groundRadianceToSurface = 0.5 * groundReflectance * (ambientLightRadianceComponent + horizonLightRadianceComponent) * normalDirectionModifier + mainLightRadianceComponent;

        // Calculate average ambient radiance - This is used in the gamut mapping process to determine the black level for compression
        inputs.averageAmbientRadiance = ambientLightIrradianceComponent[1] * (1.0 + groundReflectance);
      `),t.code.add(i`
        vec3 reflectedColorComponent = evaluateEnvironmentIllumination(inputs);
        vec3 additionalMaterialReflectanceComponent = inputs.albedoLinear * additionalAmbientIrradiance;
        vec3 outColorLinear = reflectedColorComponent + additionalMaterialReflectanceComponent;

        ${f(n,i`
        vec3 globalIlluminationEmission = 2.25 * (0.75 * inputs.albedoLinear + 0.25) * readGlobalIlluminationEmission().rgb;
        outColorLinear += globalIlluminationEmission;`)}

      ${l?i`vec3 adjustedOutColorLinear = blackLevelSoftCompression(outColorLinear, inputs.averageAmbientRadiance);`:i`vec3 adjustedOutColorLinear = max(vec3(0.0), outColorLinear - 0.005 * inputs.averageAmbientRadiance);`}

        return delinearizeGamma(adjustedOutColorLinear);
      }
    `);break}case 5:case 6:{const g=n?.35:.5,h=n?.75:1,v=n?1.5:1;de(t),ue(t),t.code.add(i`
      const float roughnessTerrain = 0.5;
      const float specularityTerrain = ${i.float(g)};

      vec3 evaluatePBRSimplifiedLighting(vec3 normal, vec3 albedo, float shadow, float ssao, vec3 additionalLight, vec3 viewDirection, vec3 upDirection) {
        PBRShadingInfo inputs;
        calculateSimplifiedInputs(inputs, normal, viewDirection, upDirection, albedo);

        ${f(n,i`vec3 globalIlluminationOcclusion = min(1.2 * readGlobalIlluminationOcclusion(), 1.0);`)}

        vec3 mainLightIrradianceComponent = ${i.float(h)} * (1.0 - shadow) * inputs.NdotL * mainLightIntensity;
        vec3 ambientLightIrradianceComponent = ${i.float(v)} * calculateAmbientIrradiance(normal) * (1.0 - ${s}) + additionalLight;
        vec3 ambientSky = ambientLightIrradianceComponent + mainLightIrradianceComponent;

        vec3 indirectDiffuse = ((1.0 - inputs.NdotUP) * mainLightIrradianceComponent + (1.0 + inputs.NdotUP ) * ambientSky) * 0.5;
        vec3 outDiffColor = inputs.albedoLinear * (1.0 - inputs.f0) * indirectDiffuse / PI;

        vec3 mainLightRadianceComponent = normalDistribution(inputs.NdotH, roughnessTerrain) * mainLightIntensity;
        vec2 dfg = prefilteredDFGAnalytical(roughnessTerrain, inputs.NdotV);
        vec3 specularColor = inputs.f0 * dfg.x + inputs.f90 * dfg.y;
        vec3 specularComponent = specularityTerrain * specularColor * mainLightRadianceComponent;

        vec3 outColorLinear = outDiffColor + specularComponent;

        ${f(n,i`
        vec3 globalIlluminationEmission = 2.25 * (0.75 * inputs.albedoLinear + 0.25) * readGlobalIlluminationEmission().rgb;
        outColorLinear += globalIlluminationEmission;`)}

        return delinearizeGamma(outColorLinear);
      }
      `);break}}}function Ji(t,e){const a=t.fragment;switch(a.code.add(i`struct ShadingNormalParameters {
vec3 normalView;
vec3 viewDirection;
} shadingParams;`),e.doubleSidedMode){case 0:a.code.add(i`vec3 shadingNormal(ShadingNormalParameters params) {
return normalize(params.normalView);
}`);break;case 1:a.code.add(i`vec3 shadingNormal(ShadingNormalParameters params) {
return dot(params.normalView, params.viewDirection) > 0.0 ? normalize(-params.normalView) : normalize(params.normalView);
}`);break;case 2:a.code.add(i`vec3 shadingNormal(ShadingNormalParameters params) {
return gl_FrontFacing ? normalize(params.normalView) : normalize(-params.normalView);
}`);break;default:e.doubleSidedMode;case 3:}}function ha(t,e){const a=e.pbrMode,o=t.fragment;switch(a){case 5:case 3:case 4:case 6:return void o.code.add(i`void applyPBRFactors() {}`);case 0:case 7:return void o.code.add(i`void applyPBRFactors() {}
float getBakedOcclusion() { return 1.0; }`);case 2:return void o.code.add(i`vec3 mrr = vec3(0.0, 0.6, 0.2);
float occlusion = 1.0;
void applyPBRFactors() {}
float getBakedOcclusion() { return 1.0; }`);case 1:{const{hasMetallicRoughnessTexture:r,hasMetallicRoughnessTextureTransform:n,hasOcclusionTexture:l,hasOcclusionTextureTransform:s,bindType:d}=e;(r||l)&&t.include(Lt,e),o.code.add(i`vec3 mrr;
float occlusion;`),r&&o.uniforms.add(d===1?new b("texMetallicRoughness",u=>u.textureMetallicRoughness):new W("texMetallicRoughness",u=>u.textureMetallicRoughness)),l&&o.uniforms.add(d===1?new b("texOcclusion",u=>u.textureOcclusion):new W("texOcclusion",u=>u.textureOcclusion)),o.uniforms.add(d===1?new ce("mrrFactors",u=>u.mrrFactors):new Qa("mrrFactors",u=>u.mrrFactors)),o.code.add(i`
        ${f(r,i`void applyMetallicRoughness(vec2 uv) {
                vec3 metallicRoughness = textureLookup(texMetallicRoughness, uv).rgb;
                mrr[0] *= metallicRoughness.b;
                mrr[1] *= metallicRoughness.g;
              }`)}

        ${f(l,"void applyOcclusion(vec2 uv) { occlusion *= textureLookup(texOcclusion, uv).r; }")}

        float getBakedOcclusion() {
          return ${l?"occlusion":"1.0"};
        }

        void applyPBRFactors() {
          mrr = mrrFactors;
          occlusion = 1.0;

          ${f(r,`applyMetallicRoughness(${n?"metallicRoughnessUV":"vuv0"});`)}
          ${f(l,`applyOcclusion(${s?"occlusionUV":"vuv0"});`)}
        }`)}}}function Yi(t,e){const a=$e(e.output)&&e.receiveShadows;a&&Ko(t,!0),t.vertex.code.add(i`
    void forwardLinearDepthToReadShadowMap() { ${f(a,"forwardLinearDepth(gl_Position.w);")} }
  `)}let Xi=class extends De{constructor(e,a,o,r){super(e,"mat4",2,(n,l,s,d)=>n.setUniformMatrices4fv(e,a(l,s,d),r),o)}},Ki=class extends De{constructor(e,a,o,r){super(e,"mat4",1,(n,l,s)=>n.setUniformMatrices4fv(e,a(l,s),r),o)}};function Qi(t){t.uniforms.add(new Ki("shadowMapMatrix",(e,a)=>a.shadowMap.getShadowMapMatrices(e.origin),4)),t.include(ma)}function er(t){t.uniforms.add(new Xi("shadowMapMatrix",(e,a)=>a.shadowMap.getShadowMapMatrices(e.origin),4)),t.include(ma)}function ma(t){t.uniforms.add(new jo("cascadeDistances",e=>e.shadowMap.cascadeDistances),new uo("numCascades",e=>e.shadowMap.numCascades)),t.code.add(tr)}const tr=i`const vec3 invalidShadowmapUVZ = vec3(0.0, 0.0, -1.0);
vec3 lightSpacePosition(vec3 _vpos, mat4 mat) {
vec4 lv = mat * vec4(_vpos, 1.0);
lv.xy /= lv.w;
return 0.5 * lv.xyz + vec3(0.5);
}
vec2 cascadeCoordinates(int i, ivec2 textureSize, vec3 lvpos) {
float xScale = float(textureSize.y) / float(textureSize.x);
return vec2((float(i) + lvpos.x) * xScale, lvpos.y);
}
vec3 calculateUVZShadow(in vec3 _worldPos, in float _linearDepth, in ivec2 shadowMapSize) {
int i = _linearDepth < cascadeDistances[1] ? 0 : _linearDepth < cascadeDistances[2] ? 1 : _linearDepth < cascadeDistances[3] ? 2 : 3;
if (i >= numCascades) {
return invalidShadowmapUVZ;
}
mat4 shadowMatrix = i == 0 ? shadowMapMatrix[0] : i == 1 ? shadowMapMatrix[1] : i == 2 ? shadowMapMatrix[2] : shadowMapMatrix[3];
vec3 lvpos = lightSpacePosition(_worldPos, shadowMatrix);
if (lvpos.z >= 1.0 || lvpos.x < 0.0 || lvpos.x > 1.0 || lvpos.y < 0.0 || lvpos.y > 1.0) {
return invalidShadowmapUVZ;
}
vec2 uvShadow = cascadeCoordinates(i, shadowMapSize, lvpos);
return vec3(uvShadow, lvpos.z);
}`;function ar(t){t.code.add(i`float readShadowMapUVZ(vec3 uvzShadow, sampler2DShadow _shadowMap) {
return texture(_shadowMap, uvzShadow);
}`)}let or=class extends De{constructor(e,a){super(e,"sampler2DShadow",0,(o,r)=>o.bindTexture(e,a(r)))}};function pa(t,e){e.receiveShadows&&t.fragment.include(Qi),va(t,e)}function fa(t,e){e.receiveShadows&&t.fragment.include(er),va(t,e)}function va(t,e){t.fragment.uniforms.add(new R("lightingGlobalFactor",n=>n.lighting.globalFactor));const{hasShadowHighlights:a,receiveShadows:o,spherical:r}=e;t.include(Yi,e),o&&ir(t.fragment,a),t.fragment.code.add(i`
    float readShadow(float additionalAmbientScale, vec3 vpos) {
      return ${o?"max(lightingGlobalFactor * (1.0 - additionalAmbientScale), readShadowMap(vpos, linearDepth))":f(r,"lightingGlobalFactor * (1.0 - additionalAmbientScale)","0.0")};
    }
  `)}function ir(t,e){nr(t,e),rr(t)}function rr(t){t.code.add(i`float readShadowMap(const in vec3 _worldPos, float _linearDepth) {
vec3 uvzShadow = calculateUVZShadow(_worldPos, _linearDepth, textureSize(shadowMap, 0));
return readShadowMaps(uvzShadow);
}`)}function nr(t,e){t.include(ar),t.uniforms.add(sr()),e&&t.uniforms.add(new O("shadowHighlight",({shadowHighlight:a})=>a?.getTexture())),t.code.add(i`
    float readShadowMaps(const in vec3 uvzShadow) {
      if (uvzShadow.z < 0.0) {
        return 0.0;
      }

      float shadow1 = readShadowMapUVZ(uvzShadow, shadowMap);
      ${f(e,`float shadow2 = texelFetch(shadowHighlight, ivec2(gl_FragCoord.xy), 0).r;
         return shadow1 > shadow2 ? shadow1 : shadow2;`,"return shadow1;")}
    }
  `)}function sr(){return new or("shadowMap",({shadowMap:t})=>t.getOutput(5)??t.getOutput(7))}function ga(t,e){e.hasColorTextureTransform?(t.varyings.add("colorUV","vec2"),t.vertex.uniforms.add(new B("colorTextureTransformMatrix",a=>a.colorTextureTransformMatrix??K)).code.add(i`void forwardColorUV(){
colorUV = (colorTextureTransformMatrix * vec3(vuv0, 1.0)).xy;
}`)):t.vertex.code.add(i`void forwardColorUV(){}`)}function lr(t,e){e.hasNormalTextureTransform&&e.textureCoordinateType!==0?(t.varyings.add("normalUV","vec2"),t.vertex.uniforms.add(new B("normalTextureTransformMatrix",a=>a.normalTextureTransformMatrix??K)).code.add(i`void forwardNormalUV(){
normalUV = (normalTextureTransformMatrix * vec3(vuv0, 1.0)).xy;
}`)):t.vertex.code.add(i`void forwardNormalUV(){}`)}function xa(t,e){e.hasEmissionTextureTransform&&e.textureCoordinateType!==0?(t.varyings.add("emissiveUV","vec2"),t.vertex.uniforms.add(new B("emissiveTextureTransformMatrix",a=>a.emissiveTextureTransformMatrix??K)).code.add(i`void forwardEmissiveUV(){
emissiveUV = (emissiveTextureTransformMatrix * vec3(vuv0, 1.0)).xy;
}`)):t.vertex.code.add(i`void forwardEmissiveUV(){}`)}function cr(t,e){e.hasOcclusionTextureTransform&&e.textureCoordinateType!==0?(t.varyings.add("occlusionUV","vec2"),t.vertex.uniforms.add(new B("occlusionTextureTransformMatrix",a=>a.occlusionTextureTransformMatrix??K)).code.add(i`void forwardOcclusionUV(){
occlusionUV = (occlusionTextureTransformMatrix * vec3(vuv0, 1.0)).xy;
}`)):t.vertex.code.add(i`void forwardOcclusionUV(){}`)}function dr(t,e){e.hasMetallicRoughnessTextureTransform&&e.textureCoordinateType!==0?(t.varyings.add("metallicRoughnessUV","vec2"),t.vertex.uniforms.add(new B("metallicRoughnessTextureTransformMatrix",a=>a.metallicRoughnessTextureTransformMatrix??K)).code.add(i`void forwardMetallicRoughnessUV(){
metallicRoughnessUV = (metallicRoughnessTextureTransformMatrix * vec3(vuv0, 1.0)).xy;
}`)):t.vertex.code.add(i`void forwardMetallicRoughnessUV(){}`)}function ba(t,e){e.snowCover&&(t.uniforms.add(new R("snowCover",a=>a.snowCover)).code.add(i`float getSnow(vec3 normal, vec3 groundNormal) {
return smoothstep(0.5, 0.55, dot(normal, groundNormal)) * snowCover;
}
float getRealisticTreeSnow(vec3 faceNormal, vec3 shadingNormal, vec3 groundNormal) {
float snow = min(1.0, smoothstep(0.5, 0.55, dot(faceNormal, groundNormal)) +
smoothstep(0.5, 0.55, dot(-faceNormal, groundNormal)) +
smoothstep(0.0, 0.1, dot(shadingNormal, groundNormal)));
return snow * snowCover;
}`),t.code.add(i`vec3 applySnowToMRR(vec3 mrr, float snow) {
return mix(mrr, vec3(0.0, 1.0, 0.04), snow);
}`))}function ya(t){const e=new V,{attributes:a,vertex:o,fragment:r,varyings:n}=e,{output:l,normalType:s,offsetBackfaces:d,spherical:u,snowCover:g,pbrMode:h,textureAlphaPremultiplied:v,instancedDoublePrecision:C,hasVertexColors:y,hasVertexTangents:I,hasColorTexture:S,hasNormalTexture:P,hasNormalTextureTransform:$,hasColorTextureTransform:T}=t;if(k(o,t),a.add("position","vec3"),o.inputs.add("position",()=>"position"),n.add("vpos","vec3",{invariant:!0}),e.include(ne,t),e.include(aa,t),e.include(At,t),e.include(ga,t),!$e(l))return e.include(oa,t),e;e.include(lr,t),e.include(xa,t),e.include(cr,t),e.include(dr,t),Se(o,t),e.include(Pe,t),e.include(Z);const _=s===0||s===1;return _&&d&&e.include(ta),e.include(oi,t),e.include(Qt,t),n.add("vPositionLocal","vec3"),e.include(U,t),e.include(rt,t),e.include(C?pa:fa,t),o.main.add(i`
    forwardDefaultMaterialExternalColor();

    vpos = getVertexInLocalOriginSpace();
    vPositionLocal = vpos - view[3].xyz;
    vpos = subtractOrigin(vpos);
    ${f(_,"vNormalWorld = dpNormal(vvLocalNormal(normalModel()));")}
    vpos = addVerticalOffset(vpos, localOrigin);
    ${f(I,"vTangent = dpTransformVertexTangent(tangent);")}
    gl_Position = transformPosition(proj, view, vpos);
    ${f(_&&d,"gl_Position = offsetBackfacingClipPosition(gl_Position, vpos, vNormalWorld, cameraPosition);")}

    forwardTextureCoordinates();
    forwardColorUV();
    forwardNormalUV();
    forwardEmissiveUV();
    forwardOcclusionUV();
    forwardMetallicRoughnessUV();

    if (shouldCullByOpacity()) {
      gl_Position = vec4(1e38, 1e38, 1e38, 1.0);
    }
    forwardLinearDepthToReadShadowMap();
  `),r.include(ua,t),r.include(nt,t),e.include(Y,t),r.include(q,t),e.include(Zt,t),Se(r,t),r.uniforms.add(o.uniforms.get("localOrigin"),new ce("ambient",D=>D.ambient),new ce("diffuse",D=>D.diffuse)),S&&r.uniforms.add(new b("tex",D=>D.texture)),e.include(ha,t),r.include(st,t),e.include(Ji,t),r.include(ba,t),gt(r),xt(r),ue(r),r.main.add(i`
    discardBySlice(vpos);
    ${S?i`
            vec4 texColor = texture(tex, ${T?"colorUV":"vuv0"});
            ${f(v,"texColor.rgb /= texColor.a;")}
            discardOrAdjustAlpha(texColor);`:i`vec4 texColor = vec4(1.0);`}
    shadingParams.viewDirection = normalize(vpos - cameraPosition);
    ${s===2?i`vec3 normal = screenDerivativeNormal(vPositionLocal);`:i`shadingParams.normalView = vNormalWorld;
                vec3 normal = shadingNormal(shadingParams);`}
    applyPBRFactors();
    float ssao = evaluateAmbientOcclusionInverse() * getBakedOcclusion();

    vec3 posWorld = vpos + localOrigin;

    float additionalAmbientScale = additionalDirectedAmbientLight(posWorld);
    float shadow = readShadow(additionalAmbientScale, vpos);

    vec3 matColor = max(ambient, diffuse);
    vec3 albedo = mixExternalColor(
      ${f(y,"vColor.rgb *")} matColor,
      texColor.rgb,
      vExternalColor.rgb,
      colorMixMode
    );
    float opacity_ = getDefaultMaterialOpacity(texColor.a);

    ${P?`mat3 tangentSpace = computeTangentSpace(${I?"normal":"normal, vpos, vuv0"});
           vec3 shadingNormal = computeTextureNormal(tangentSpace, ${$?"normalUV":"vuv0"});`:"vec3 shadingNormal = normal;"}
    vec3 normalGround = ${u?"normalize(posWorld);":"vec3(0.0, 0.0, 1.0);"}

    ${f(g,i`
          float snow = getSnow(normal, normalGround);
          albedo = mix(albedo, vec3(1), snow);
          shadingNormal = mix(shadingNormal, normal, snow);
          ssao = mix(ssao, 1.0, snow);`)}

    vec3 additionalLight = ssao * mainLightIntensity * additionalAmbientScale * ambientBoostFactor * lightingGlobalFactor;

    ${h===1||h===2?i`
            float additionalAmbientIrradiance = additionalAmbientIrradianceFactor * mainLightIntensity[2];
            ${f(g,"mrr = applySnowToMRR(mrr, snow);")}
            vec3 shadedColor = evaluateSceneLightingPBR(shadingNormal, albedo, shadow, 1.0 - ssao, additionalLight, shadingParams.viewDirection, normalGround, mrr, additionalAmbientIrradiance);`:h===7?i`vec3 shadedColor = albedo;`:i`vec3 shadedColor = evaluateSceneLighting(shadingNormal, albedo, shadow, 1.0 - ssao, additionalLight);`}
    vec4 finalColor = vec4(shadedColor, opacity_);
    outputColorHighlightOLID(applySlice(finalColor, vpos), albedo ${f(g,", snow")});
  `),e}const ur=Object.freeze(Object.defineProperty({__proto__:null,build:ya},Symbol.toStringTag,{value:"Module"}));class hr extends Lo{constructor(){super(...arguments),this.isSchematic=!1,this.usePBR=!1,this.mrrFactors=Yo,this.hasVertexColors=!1,this.hasSymbolColors=!1,this.doubleSided=!1,this.doubleSidedType="normal",this.cullFace=2,this.instanced=!1,this.instancedFeatureAttribute=!1,this.instancedColor=!1,this.instanceColorEncodesAlphaIgnore=!1,this.emissiveStrengthFromSymbol=0,this.emissiveStrengthKHR=1,this.emissiveSource=1,this.emissiveBaseColor=Xe,this.instancedDoublePrecision=!1,this.normalType=0,this.receiveShadows=!0,this.receiveAmbientOcclusion=!0,this.castShadows=!0,this.ambient=le(.2,.2,.2),this.diffuse=le(.8,.8,.8),this.externalColor=Ea(1,1,1,1),this.colorMixMode="multiply",this.opacity=1,this.layerOpacity=1,this.origin=w(),this.hasSlicePlane=!1,this.offsetTransparentBackfaces=!1,this.vvSize=null,this.vvColor=null,this.vvOpacity=null,this.modelTransformation=null,this.drivenOpacity=!1,this.customDepthTest=0,this.textureAlphaMode=0,this.textureAlphaCutoff=Jt,this.textureAlphaPremultiplied=!1,this.renderOccluded=1,this.testsTransparentRenderOrder=0,this.isDecoration=!1}get hasVVSize(){return!!this.vvSize}get hasVVColor(){return!!this.vvColor}get hasVVOpacity(){return!!this.vvOpacity}}let Sn=class extends Vo{constructor(){super(...arguments),this.origin=w(),this.slicePlaneLocalOrigin=this.origin}},Ce=class extends ee{constructor(t,e){let a=Ae(wa(e));e.instanced&&e.instancedDoublePrecision&&(a=a.concat(Ae(Jo(e)))),super(t,e,a),this.shader=new L(ur,()=>E(()=>Promise.resolve().then(()=>zr),void 0)),this.ignoreUnused=!0}_makePipeline(t,e){const{output:a,transparent:o,cullFace:r,customDepthTest:n,hasOccludees:l}=t;return te({blending:o?po(a,!1,t.emissionDimmingPass):null,culling:pr(t)?Bo(r):null,depthTest:xo(a,mr(n)),depthWrite:mo(t),colorWrite:ae,stencilWrite:l?go:null,stencilTest:l?e?fo:vo:null,polygonOffset:ho(t)})}initializePipeline(t){return this._occludeePipelineState=this._makePipeline(t,!0),this._makePipeline(t,!1)}getPipeline(t,e,a){return a?this._occludeePipelineState:super.getPipeline(t,e,a)}};function mr(t){switch(t){case 1:return 515;case 0:case 3:return 513;case 2:return 516}}function pr(t){return t.cullFace!==0||!t.hasSlicePlane&&!t.transparent&&!t.doubleSidedMode}function wa(t){const e=it().vec3f("position");return t.normalType===1?e.vec2i16("normalCompressed",{glNormalized:!0}):e.vec3f("normal"),t.hasVertexTangents&&e.vec4f("tangent"),t.hasTextures&&e.vec2f16("uv0"),t.hasVertexColors&&e.vec4u8("color",{glNormalized:!0}),t.hasSymbolColors&&e.vec4u8("symbolColor"),!t.instanced&&Ht()&&e.vec4u8("olidColor"),e}Ce=c([j("esri.views.3d.webgl-engine.shaders.DefaultMaterialTechnique")],Ce);class p extends bo{constructor(e){super(),this.spherical=e,this.alphaDiscardMode=1,this.doubleSidedMode=0,this.pbrMode=0,this.cullFace=0,this.normalType=0,this.customDepthTest=0,this.emissionSource=0,this.hasVertexColors=!1,this.hasSymbolColors=!1,this.hasVerticalOffset=!1,this.hasColorTexture=!1,this.hasMetallicRoughnessTexture=!1,this.hasOcclusionTexture=!1,this.hasNormalTexture=!1,this.hasScreenSizePerspective=!1,this.hasVertexTangents=!1,this.hasOccludees=!1,this.instanced=!1,this.instancedDoublePrecision=!1,this.hasModelTransformation=!1,this.offsetBackfaces=!1,this.hasVVSize=!1,this.hasVVColor=!1,this.receiveShadows=!1,this.hasShadowHighlights=!1,this.receiveAmbientOcclusion=!1,this.receiveGlobalIllumination=!1,this.textureAlphaPremultiplied=!1,this.instancedFeatureAttribute=!1,this.instancedColor=!1,this.writeDepth=!0,this.snowCover=!1,this.hasColorTextureTransform=!1,this.hasEmissionTextureTransform=!1,this.hasNormalTextureTransform=!1,this.hasOcclusionTextureTransform=!1,this.hasMetallicRoughnessTextureTransform=!1,this.useCustomDTRExponentForWater=!1,this.useFillLights=!0,this.draped=!1,this.renderOccluded=!1}get textureCoordinateType(){return this.hasTextures?1:0}get hasTextures(){return this.hasColorTexture||this.hasNormalTexture||this.hasMetallicRoughnessTexture||this.emissionSource===3||this.hasOcclusionTexture}get hasVVInstancing(){return this.instanced}}c([m({count:4})],p.prototype,"alphaDiscardMode",void 0),c([m({count:3})],p.prototype,"doubleSidedMode",void 0),c([m({count:8})],p.prototype,"pbrMode",void 0),c([m({count:3})],p.prototype,"cullFace",void 0),c([m({count:3})],p.prototype,"normalType",void 0),c([m({count:3})],p.prototype,"customDepthTest",void 0),c([m({count:8})],p.prototype,"emissionSource",void 0),c([m()],p.prototype,"hasVertexColors",void 0),c([m()],p.prototype,"hasSymbolColors",void 0),c([m()],p.prototype,"hasVerticalOffset",void 0),c([m()],p.prototype,"hasColorTexture",void 0),c([m()],p.prototype,"hasMetallicRoughnessTexture",void 0),c([m()],p.prototype,"hasOcclusionTexture",void 0),c([m()],p.prototype,"hasNormalTexture",void 0),c([m()],p.prototype,"hasScreenSizePerspective",void 0),c([m()],p.prototype,"hasVertexTangents",void 0),c([m()],p.prototype,"hasOccludees",void 0),c([m()],p.prototype,"instanced",void 0),c([m()],p.prototype,"instancedDoublePrecision",void 0),c([m()],p.prototype,"hasModelTransformation",void 0),c([m()],p.prototype,"offsetBackfaces",void 0),c([m()],p.prototype,"hasVVSize",void 0),c([m()],p.prototype,"hasVVColor",void 0),c([m()],p.prototype,"receiveShadows",void 0),c([m()],p.prototype,"hasShadowHighlights",void 0),c([m()],p.prototype,"receiveAmbientOcclusion",void 0),c([m()],p.prototype,"receiveGlobalIllumination",void 0),c([m()],p.prototype,"textureAlphaPremultiplied",void 0),c([m()],p.prototype,"instancedFeatureAttribute",void 0),c([m()],p.prototype,"instancedColor",void 0),c([m()],p.prototype,"writeDepth",void 0),c([m()],p.prototype,"snowCover",void 0),c([m()],p.prototype,"hasColorTextureTransform",void 0),c([m()],p.prototype,"hasEmissionTextureTransform",void 0),c([m()],p.prototype,"hasNormalTextureTransform",void 0),c([m()],p.prototype,"hasOcclusionTextureTransform",void 0),c([m()],p.prototype,"hasMetallicRoughnessTextureTransform",void 0);function Ma(t){const e=new V,{attributes:a,vertex:o,fragment:r,varyings:n}=e,{output:l,offsetBackfaces:s,pbrMode:d,snowCover:u,spherical:g}=t,h=d===1||d===2;if(k(o,t),a.add("position","vec3"),o.inputs.add("position",()=>"position"),n.add("vpos","vec3",{invariant:!0}),e.include(ne,t),e.include(aa,t),e.include(At,t),e.include(ga,t),!$e(l))return e.include(oa,t),e;e.include(xa,t),Se(e.vertex,t),e.include(Pe,t),e.include(Z),s&&e.include(ta),n.add("vNormalWorld","vec3"),n.add("localvpos","vec3",{invariant:!0}),e.include(U,t),e.include(rt,t),e.include(t.instancedDoublePrecision?pa:fa,t),o.main.add(i`
    forwardDefaultMaterialExternalColor();

    vpos = getVertexInLocalOriginSpace();

    localvpos = vpos - view[3].xyz;
    vpos = subtractOrigin(vpos);
    vNormalWorld = dpNormal(vvLocalNormal(normalModel()));
    vpos = addVerticalOffset(vpos, localOrigin);
    vec4 basePosition = transformPosition(proj, view, vpos);

    forwardTextureCoordinates();
    forwardColorUV();
    forwardEmissiveUV();
    forwardLinearDepthToReadShadowMap();

    gl_Position = shouldCullByOpacity() ? vec4(1e38, 1e38, 1e38, 1.0) :
    ${f(s,"offsetBackfacingClipPosition(basePosition, vpos, vNormalWorld, cameraPosition);","basePosition;")}
  `);const{hasColorTexture:v,hasColorTextureTransform:C}=t;return r.include(ua,t),r.include(nt,t),e.include(Y,t),r.include(q,t),e.include(Zt,t),Se(r,t),de(r),gt(r),xt(r),r.uniforms.add(o.uniforms.get("localOrigin"),o.uniforms.get("view"),new ce("ambient",y=>y.ambient),new ce("diffuse",y=>y.diffuse)),v&&r.uniforms.add(new b("tex",y=>y.texture)),e.include(ha,t),r.include(st,t),r.include(ba,t),ue(r),r.main.add(i`
      discardBySlice(vpos);
      vec4 texColor = ${v?`texture(tex, ${C?"colorUV":"vuv0"})`:" vec4(1.0)"};
      ${f(v,`${f(t.textureAlphaPremultiplied,"texColor.rgb /= texColor.a;")}
        discardOrAdjustAlpha(texColor);`)}
      vec3 viewDirection = normalize(vpos - cameraPosition);
      applyPBRFactors();
      float ssao = evaluateAmbientOcclusionInverse();
      ssao *= getBakedOcclusion();

      float additionalAmbientScale = additionalDirectedAmbientLight(vpos + localOrigin);
      vec3 additionalLight = ssao * mainLightIntensity * additionalAmbientScale * ambientBoostFactor * lightingGlobalFactor;
      float shadow = readShadow(additionalAmbientScale, vpos);
      vec3 matColor = max(ambient, diffuse);
      vec3 albedo = mixExternalColor(
        ${f(t.hasVertexColors,"vColor.rgb * ")} matColor,
        texColor.rgb,
        vExternalColor.rgb,
        colorMixMode
      );
      float opacity_ = getDefaultMaterialOpacity(texColor.a);

      vec3 shadingNormal = normalize(vNormalWorld);
      vec3 groundNormal = ${g?"normalize(vpos + localOrigin)":"vec3(0.0, 0.0, 1.0)"};

      ${f(u,`vec3 faceNormal = screenDerivativeNormal(vpos);
         float snow = getRealisticTreeSnow(faceNormal, shadingNormal, groundNormal);
         albedo = mix(albedo, vec3(1), snow);`)}

      ${i`albedo *= 1.2;
             vec3 viewForward = vec3(view[0][2], view[1][2], view[2][2]);
             float alignmentLightView = clamp(dot(viewForward, -mainLightDirection), 0.0, 1.0);
             float transmittance = 1.0 - clamp(dot(viewForward, shadingNormal), 0.0, 1.0);
             float treeRadialFalloff = vColor.r;
             float backLightFactor = 0.5 * treeRadialFalloff * alignmentLightView * transmittance * (1.0 - shadow);
             additionalLight += backLightFactor * mainLightIntensity;`}

      ${h?i`float additionalAmbientIrradiance = additionalAmbientIrradianceFactor * mainLightIntensity[2];
            ${f(u,"mrr = applySnowToMRR(mrr, snow);")}
            vec3 shadedColor = evaluateSceneLightingPBR(shadingNormal, albedo, shadow, 1.0 - ssao, additionalLight, viewDirection, groundNormal, mrr, additionalAmbientIrradiance);`:i`vec3 shadedColor = evaluateSceneLighting(shadingNormal, albedo, shadow, 1.0 - ssao, additionalLight);`}
      vec4 finalColor = vec4(shadedColor, opacity_);
      outputColorHighlightOLID(applySlice(finalColor, vpos), albedo ${f(u,", 1.0")});`),e}const fr=Object.freeze(Object.defineProperty({__proto__:null,build:Ma},Symbol.toStringTag,{value:"Module"}));let Ye=class extends Ce{constructor(){super(...arguments),this.shader=new L(fr,()=>E(()=>Promise.resolve().then(()=>Pr),void 0))}};Ye=c([j("esri.views.3d.webgl-engine.shaders.RealisticTreeTechnique")],Ye);class vr extends Vt{constructor(){super(...arguments),this.receiveShadows=!0}}function gr(t,e){return t.receiveShadows&&e.shadowHighlight?.getTexture()!=null}c([m()],vr.prototype,"receiveShadows",void 0);class Cn extends yo{constructor(e,a){super(e,br),this.materialType="default",this.supportsEdges=!0,this.intersectRayDraped=void 0,this.intersectScreenPolygonDraped=void 0,this.produces=new Map([[2,o=>St(o)&&!this.transparent],[4,o=>St(o)&&this.transparent]]),this._layout=wa(this.parameters),this._configuration=new p(a.spherical)}isVisibleForOutput(e){return e!==5&&e!==7&&e!==6||this.parameters.castShadows}get visible(){const{layerOpacity:e,colorMixMode:a,opacity:o,externalColor:r}=this.parameters;return e*(a==="replace"?1:o)*(a==="ignore"||isNaN(r[3])?1:r[3])>=Jt}get _hasEmissiveBase(){return!!this.parameters.emissiveTextureId||!Gt(this.parameters.emissiveBaseColor,Xe)}get emissions(){return this.parameters.emissiveStrength>0&&(this.parameters.emissiveSource===0&&this._hasEmissiveBase||this.parameters.emissiveSource===1)?this.transparent?2:1:0}updateConfiguration(e){super.updateConfiguration(e);const{parameters:a,_configuration:o}=this;o.hasNormalTexture=a.hasNormalTexture,o.hasColorTexture=a.hasColorTexture,o.hasMetallicRoughnessTexture=a.hasMetallicRoughnessTexture,o.hasOcclusionTexture=a.hasOcclusionTexture;const{treeRendering:r,doubleSided:n,doubleSidedType:l}=a;o.hasVertexTangents=!r&&a.hasVertexTangents,o.instanced=a.instanced,o.instancedDoublePrecision=a.instancedDoublePrecision,o.hasVVColor=!!a.vvColor,o.hasVVSize=!!a.vvSize,o.hasVerticalOffset=a.verticalOffset!=null,o.hasScreenSizePerspective=a.screenSizePerspective!=null,o.hasSlicePlane=a.hasSlicePlane,o.alphaDiscardMode=a.textureAlphaMode,o.normalType=r?0:a.normalType,o.transparent=this.transparent,o.enableOITOffset=e.enableOITOffset,o.customDepthTest=a.customDepthTest??0,o.hasOccludees=e.hasOccludees,o.cullFace=a.hasSlicePlane?0:a.cullFace,o.hasModelTransformation=!r&&a.modelTransformation!=null,o.hasVertexColors=a.hasVertexColors,o.hasSymbolColors=a.hasSymbolColors,o.doubleSidedMode=r?2:n&&l==="normal"?1:n&&l==="winding-order"?2:0,o.instancedFeatureAttribute=a.instancedFeatureAttribute,o.instancedColor=a.instancedColor,$e(e.output)?(o.receiveShadows=a.receiveShadows,o.hasShadowHighlights=gr(o,e),o.receiveAmbientOcclusion=a.receiveAmbientOcclusion&&e.ssao!=null,o.receiveGlobalIllumination=a.receiveAmbientOcclusion&&e.globalIlluminationEnabled):o.receiveShadows=o.hasShadowHighlights=o.receiveAmbientOcclusion=!1,o.textureAlphaPremultiplied=!!a.textureAlphaPremultiplied,o.pbrMode=a.usePBR?a.unlit?7:a.isSchematic?2:1:0,o.emissionSource=a.emissionSource,o.offsetBackfaces=!(!this.transparent||!a.offsetTransparentBackfaces),o.snowCover=e.snowCover>0,o.hasColorTextureTransform=!!a.colorTextureTransformMatrix,o.hasNormalTextureTransform=!!a.normalTextureTransformMatrix,o.hasEmissionTextureTransform=!!a.emissiveTextureTransformMatrix,o.hasOcclusionTextureTransform=!!a.occlusionTextureTransformMatrix,o.hasMetallicRoughnessTextureTransform=!!a.metallicRoughnessTextureTransformMatrix}intersectRay(e,a,o,r,n,l){const s=this._getMaterialVerticalOffset(a,o);s!=null&&(r=Be(yr,r,s),n=Be(wr,n,s)),l=Co(l,this._configuration,r,n),Io(e,o,r,n,Oo(o.verticalOffset),l)}intersectScreenPolygon(e,a,o,r){return Do(wo(e,o,a,r,this._createScreenPolygonVertexDisplacement(a,o)),this._configuration,o.camera.eye)}createGLMaterial(e){return new xr(e)}createBufferWriter(){return new $o(this._layout)}get transparent(){const{drivenOpacity:e,opacity:a,externalColor:o,layerOpacity:r,texture:n,textureId:l,textureAlphaMode:s,colorMixMode:d}=this.parameters,u=o[3];return e||a<1&&d!=="replace"||u<1&&d!=="ignore"||r<1||(n!=null||l!=null)&&s!==1&&s!==2&&d!=="replace"}_createScreenPolygonVertexDisplacement(e,a){const o=this._getMaterialVerticalOffset(e,a);return o==null?null:{applyToVertex:(r,n,l,s)=>(r[0]=n+o[0],r[1]=l+o[1],r[2]=s+o[2],r),applyToAabb:r=>(La(r,wt(be,Va(r,be),o)),Aa(r,wt(be,Ha(r,be),o)),r)}}_getMaterialVerticalOffset(e,a){if(this.parameters.verticalOffset==null)return null;const o=a.camera;se(Ve,e[12],e[13],e[14]);let r=null;switch(a.viewingMode){case 1:r=ka(Rt,Ve);break;case 2:r=Ua(Rt,Mr)}const n=Be(Sr,Ve,o.eye),l=qa(n),s=Mt(n,n,1/l);let d=null;this.parameters.screenSizePerspective&&(d=Za(r,s));const u=Mo(o,l,this.parameters.verticalOffset,d??0,this.parameters.screenSizePerspective,null);return Mt(r,r,u),Ja(Tr,r,a.transform.inverseRotation)}}class xr extends ao{constructor(e){super({...e,...e.material.parameters})}beginSlot(e){this._material.setParameters({receiveShadows:e.shadowMap.enabled});const a=this._material.parameters;this.updateTexture(a.textureId);const o=e.camera.viewInverseTransposeMatrix;return se(a.origin,o[3],o[7],o[11]),this._material.setParameters(this.textureBindParameters),this.getTechnique(a.treeRendering?Ye:Ce,e)}}class br extends hr{constructor(){super(...arguments),this.treeRendering=!1,this.useIndexing=!1,this.hasVertexTangents=!1,this.unlit=!1}get hasNormalTexture(){return!this.treeRendering&&!!this.normalTextureId}get hasColorTexture(){return!!this.textureId}get hasMetallicRoughnessTexture(){return!this.treeRendering&&!!this.metallicRoughnessTextureId}get hasOcclusionTexture(){return!this.treeRendering&&!!this.occlusionTextureId}get emissiveStrength(){return this.emissiveStrengthFromSymbol*this.emissiveStrengthKHR}get emissionSource(){return this.emissiveTextureId!=null&&this.emissiveSource===0?3:this.emissiveSource===0?2:1}get hasTextures(){return this.hasColorTexture||this.hasNormalTexture||this.hasMetallicRoughnessTexture||this.emissionSource===3||this.hasOcclusionTexture}}const yr=w(),wr=w(),Mr=Ya(0,0,1),Rt=w(),Ve=w(),Sr=w(),Tr=w(),be=w(),_r=Object.freeze(Object.defineProperty({__proto__:null,build:ra},Symbol.toStringTag,{value:"Module"})),Cr=Object.freeze(Object.defineProperty({__proto__:null,build:ia,getRadius:Fe},Symbol.toStringTag,{value:"Module"})),Ir=Object.freeze(Object.defineProperty({__proto__:null,GlobalIlluminationBlurDrawParameters:ft,build:la},Symbol.toStringTag,{value:"Module"})),Dr=Object.freeze(Object.defineProperty({__proto__:null,GlobalIlluminationPassParameters:pt,build:na,defaultColorBleedWeight:ct,defaultRayMarchMaxReach:je,defaultRayMarchMaxReachEmissionWeight:mt,defaultRayMarchMaxSteps:Oe,defaultRayMarchMinReach:ut,defaultRayMarchMinReachEmissionWeight:ht,defaultRayMarchWorldReach:dt},Symbol.toStringTag,{value:"Module"})),$r=Object.freeze(Object.defineProperty({__proto__:null,GlobalIlluminationUpscaleDrawParameters:vt,build:da},Symbol.toStringTag,{value:"Module"})),zr=Object.freeze(Object.defineProperty({__proto__:null,build:ya},Symbol.toStringTag,{value:"Module"})),Pr=Object.freeze(Object.defineProperty({__proto__:null,build:Ma},Symbol.toStringTag,{value:"Module"}));export{Cn as B,oe as F,pr as M,Sn as V,Jo as a,Qr as b,ta as c,nt as d,ti as e,gt as f,xt as g,ba as h,on as i,ue as j,gr as k,de as l,fa as m,ha as n,Yo as o,mn as p,xi as q,Ji as r,Zi as s,tn as t,an as u,ua as v,ri as w,Dt as x,pi as y};
