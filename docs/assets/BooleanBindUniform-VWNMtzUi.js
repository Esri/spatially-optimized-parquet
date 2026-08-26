import{h$ as a,mB as g,cu as u,ix as x,a as _,kp as d,ha as p}from"./index-lasFETI3.js";import{e as w,d as O,a as z,b as m}from"./OutputColorHighlightOLID.glsl-CT0JXUQY.js";import{c as $}from"./NoParameters-DB-WZ6gy.js";import{i as v,t as i}from"./getEmissions.glsl-DLg9S9NB.js";import{s as P,i as T,e as M}from"./SceneLighting-WYYKqd35.js";let E=class extends w{constructor(e){super(e),this._numLoading=0,this._disposed=!1,this._textures=e.textures,this.updateTexture(e.textureId),this._acquire(e.normalTextureId,r=>this._textureNormal=r),this._acquire(e.emissiveTextureId,r=>this._textureEmissive=r),this._acquire(e.occlusionTextureId,r=>this._textureOcclusion=r),this._acquire(e.metallicRoughnessTextureId,r=>this._textureMetallicRoughness=r)}dispose(){super.dispose(),this._texture=a(this._texture),this._textureNormal=a(this._textureNormal),this._textureEmissive=a(this._textureEmissive),this._textureOcclusion=a(this._textureOcclusion),this._textureMetallicRoughness=a(this._textureMetallicRoughness),this._disposed=!0}ensureResources(e){return this._numLoading===0?2:1}get textureBindParameters(){return new S(this._texture?.texture??null,this._textureNormal?.texture??null,this._textureEmissive?.texture??null,this._textureOcclusion?.texture??null,this._textureMetallicRoughness?.texture??null)}updateTexture(e){this._texture!=null&&e===this._texture.id||(this._texture=a(this._texture),this._acquire(e,r=>this._texture=r))}_acquire(e,r){if(e==null)return void r(null);const s=this._textures.acquire(e);if(g(s))return++this._numLoading,void s.then(o=>{if(this._disposed)return a(o),void r(null);r(o)}).finally(()=>--this._numLoading);r(s)}};class N extends ${constructor(e=null){super(),this.textureEmissive=e}}let S=class extends N{constructor(e,r,s,o,n,l,c){super(s),this.texture=e,this.textureNormal=r,this.textureOcclusion=o,this.textureMetallicRoughness=n,this.scale=l,this.normalTextureTransformMatrix=c}};class A extends v{constructor(e,r,s){super(e,"mat4",1,(o,n,l)=>o.setUniformMatrix4fv(e,r(n,l),s))}}let B=class{constructor(e){this.screenLength=_(e.screenLength),this.minWorldLength=e.minWorldLength??0,this.maxWorldLength=e.maxWorldLength??1/0}};function k(t,e){const r=t.vertex;e.hasVerticalOffset?(D(r),e.hasScreenSizePerspective&&(t.include(P),T(r),O(t.vertex,e)),r.code.add(i`
      vec3 calculateVerticalOffset(vec3 worldPos, vec3 localOrigin) {
        float viewDistance = length((view * vec4(worldPos, 1.0)).xyz);
        ${e.spherical?i`vec3 worldNormal = normalize(worldPos + localOrigin);`:i`vec3 worldNormal = vec3(0.0, 0.0, 1.0);`}
        ${e.hasScreenSizePerspective?i`
            float cosAngle = dot(worldNormal, normalize(worldPos - cameraPosition));
            float verticalOffsetScreenHeight = screenSizePerspectiveScaleFloat(verticalOffset.x, abs(cosAngle), viewDistance, screenSizePerspectiveAlignment);`:i`
            float verticalOffsetScreenHeight = verticalOffset.x;`}
        // Screen sized offset in world space, used for example for line callouts
        float worldOffset = clamp(verticalOffsetScreenHeight * verticalOffset.y * viewDistance, verticalOffset.z, verticalOffset.w);
        return worldNormal * worldOffset;
      }

      vec3 addVerticalOffset(vec3 worldPos, vec3 localOrigin) {
        return worldPos + calculateVerticalOffset(worldPos, localOrigin);
      }
    `)):r.code.add(i`vec3 addVerticalOffset(vec3 worldPos, vec3 localOrigin) { return worldPos; }`)}const j=x();function D(t){t.uniforms.add(new z("verticalOffset",(e,r)=>{const{minWorldLength:s,maxWorldLength:o,screenLength:n}=e.verticalOffset,l=Math.tan(.5*r.camera.fovY)/(.5*r.camera.fullViewport[3]),c=r.camera.pixelRatio||1;return u(j,n*c,l,s,o)}))}function Y(t){t.uniforms.add(new m("zProjectionMap",e=>L(e.camera))),t.code.add(i`float linearizeDepth(float depth, vec2 zProjectionConstants) {
float depthNdc = depth * 2.0 - 1.0;
return -(zProjectionConstants[0] / (depthNdc + zProjectionConstants[1] + 1e-7));
}
float linearizeDepth(float depth) {
return linearizeDepth(depth, zProjectionMap);
}`),t.code.add(i`float delinearizeDepth(float linearDepth) {
float c1 = zProjectionMap[0];
float c2 = zProjectionMap[1];
float depthNdc = (-c1/linearDepth) - c2 - 1e-7;
float depthNonlinear01 = (depthNdc + 1.0 ) / 2.0;
return depthNonlinear01;
}`),t.code.add(i`float depthFromTexture(sampler2D depthTexture, vec2 uv) {
ivec2 iuv = ivec2(uv * vec2(textureSize(depthTexture, 0)));
return texelFetch(depthTexture, iuv, 0).r;
}`),t.code.add(i`float linearDepthFromTexture(sampler2D depthTexture, vec2 uv) {
return linearizeDepth(depthFromTexture(depthTexture, uv));
}`)}function L(t){const e=t.projectionMatrix;return d(I,e[14],e[10])}const I=p();function G(t){t.fragment.uniforms.add(new M("projInfo",e=>R(e.camera))),t.fragment.uniforms.add(new m("zScale",e=>W(e.camera))),t.fragment.code.add(i`vec3 reconstructPosition(vec2 fragCoord, float depth) {
return vec3((fragCoord * projInfo.xy + projInfo.zw) * (zScale.x * depth + zScale.y), depth);
}`)}function R(t){const e=t.projectionMatrix;return e[11]===0?u(h,2/(t.fullWidth*e[0]),2/(t.fullHeight*e[5]),(1+e[12])/e[0],(1+e[13])/e[5]):u(h,-2/(t.fullWidth*e[0]),-2/(t.fullHeight*e[5]),(1-e[8])/e[0],(1-e[9])/e[5])}const h=x();function W(t){return t.projectionMatrix[11]===0?d(f,0,1):d(f,1,0)}const f=p();class J extends v{constructor(e,r){super(e,"bool",0,(s,o)=>s.setUniform1b(e,r(o)))}}export{Y as a,E as b,G as c,D as d,B as e,L as i,S as l,k as n,J as r,A as t};
