import{fj as Fe,iQ as Yt,cq as zt,o as A,cm as Xt,cd as Zt,cp as Qt,qO as Kt,pm as ei,cu as ti,ix as ii,bT as le,kp as Ae,ha as Me,yG as Lt,ac as L,j0 as _t,_ as ni,k7 as Ue,af as ai,w as He,rX as Ct,eW as Z,eS as Q,cv as ye,eU as $e,eV as Ge,cK as Re,eX as qe,he as ce,eT as Ye,xC as ri,gi as Ot,cy as Xe,dQ as Ze,pb as oi,rY as K,h6 as Et,eo as si,g7 as li}from"./index-lasFETI3.js";import{f as ci}from"./computeTranslationToOriginAndRotation-DUiWNZd9.js";import{t as di,a as pi,b as de,d as ui,W as fi}from"./WebGLLayer-Z_IbLXDb.js";import{t as hi}from"./Attribute-DGhdp5lO.js";import{d as Ft,a9 as mi,aa as Qe,u as gi,J as xe,L as je,a as Ie,l as At,f as vi,a4 as Si,b as xi,h as bi,g as yi,k as $i,M as Ti,n as wi,t as Di,z as Ke,o as Pi,r as et,ab as pe,q as zi,v as Li,w as _i,ac as Ci,ad as tt,ae as Oi,af as Ei,ag as it,A as Fi,s as Ai,B as nt,ah as Ri,ai as at,T as Ii,I as Wi,aj as Vi,C as Ni,a2 as Mi,e as ji,m as ki}from"./OutputColorHighlightOLID.glsl-CT0JXUQY.js";import{r as rt,i as H,U as ot,w as Bi,I as te,y as st,S as Ji,Z as Ui,j as Hi}from"./BufferView-XM-TXjbv.js";import{h as ie,l as Gi,x as qi,M as Yi,j as Xi,v as Rt}from"./lineSegment-DNL9grwG.js";import{S as ue,n as k,E as lt,j as be}from"./plane-geaygzL4.js";import{r as j,t as a,j as ne,n as F,e as Zi,c as Qi,a as _,i as Ki,u as en,p as Te,g as tn,v as ct,o as nn}from"./getEmissions.glsl-DLg9S9NB.js";import{Q as an,t as rn}from"./InterleavedLayout-CD2RdENJ.js";import{O as fe,g as we,a as dt}from"./renderState-BhR1EdRH.js";import{s as on,t as sn,n as ln,c as cn,f as dn,e as pn}from"./SceneLighting-WYYKqd35.js";import{u as pt,s as un}from"./ShaderBuilder-B0f4Qr9Q.js";import{i as fn}from"./TextureBackedBufferLayout-5Tf-J99Q.js";function Va(t,e,i,n,r,o,c,s,l,d,p){const m=yn[p.mode];let v,f,h=0;if(Fe(t,e,i,n,l.spatialReference,r,s))return m?.requiresAlignment(p)?(h=m.applyElevationAlignmentBuffer(n,r,o,c,s,l,d,p),v=o,f=c):(v=n,f=r),Fe(v,l.spatialReference,f,o,d.spatialReference,c,s)?h:void 0}function It(t,e,i,n,r){const o=(di(t)?t.z:pi(t)?t.array[t.offset+2]:t[2])||0;switch(i.mode){case"on-the-ground":{const c=de(e,t,"ground")??0;return r.verticalDistanceToGround=0,r.sampledElevation=c,void(r.z=c)}case"relative-to-ground":{const c=de(e,t,"ground")??0,s=i.geometryZWithOffset(o,n);return r.verticalDistanceToGround=s,r.sampledElevation=c,void(r.z=s+c)}case"relative-to-scene":{const c=de(e,t,"scene")??0,s=i.geometryZWithOffset(o,n);return r.verticalDistanceToGround=s,r.sampledElevation=c,void(r.z=s+c)}case"absolute-height":{const c=i.geometryZWithOffset(o,n),s=de(e,t,"ground")??0;return r.verticalDistanceToGround=c-s,r.sampledElevation=s,void(r.z=c)}default:return void(r.z=0)}}function Na(t,e,i,n){return It(t,e,i,n,ee),ee.z}function Ma(t,e,i){return e==="on-the-ground"&&i==="on-the-ground"?t.staysOnTheGround:e===i||e!=="on-the-ground"&&i!=="on-the-ground"?e==null||i==null?t.definedChanged:1:t.onTheGroundChanged}function ja(t){return t==="relative-to-ground"||t==="relative-to-scene"}function ka(t){return t!=="absolute-height"}function Ba(t,e,i,n,r){It(e,i,r,n,ee),hn(t,ee.verticalDistanceToGround);const o=ee.sampledElevation,c=Yt($n,t.transformation);return he[0]=e.x,he[1]=e.y,he[2]=ee.z,ci(e.spatialReference,he,c,n.spatialReference)?t.transformation=c:console.warn("Could not locate symbol object properly, it might be misplaced"),o}function hn(t,e){for(let i=0;i<t.geometries.length;++i){const n=t.geometries[i].getMutableAttribute("groundDistance");n&&n.data[0]!==e&&(n.data[0]=e,t.geometryVertexAttributeUpdated(t.geometries[i],"groundDistance"))}}function mn(t,e,i,n,r,o){let c=0;const s=o.spatialReference;e*=3,n*=3;for(let l=0;l<r;++l){const d=t[e],p=t[e+1],m=t[e+2],v=o.getElevation(d,p,m,s,"ground")??0;c+=v,i[n]=d,i[n+1]=p,i[n+2]=v,e+=3,n+=3}return c/r}function gn(t,e,i,n,r,o,c,s){let l=0;const d=s.calculateOffsetRenderUnits(c),p=s.featureExpressionInfoContext,m=o.spatialReference;e*=3,n*=3;for(let v=0;v<r;++v){const f=t[e],h=t[e+1],$=t[e+2],S=o.getElevation(f,h,$,m,"ground")??0;l+=S,i[n]=f,i[n+1]=h,i[n+2]=p==null?$+S+d:S+d,e+=3,n+=3}return l/r}function vn(t,e,i,n,r,o,c,s){let l=0;const d=s.calculateOffsetRenderUnits(c),p=s.featureExpressionInfoContext,m=o.spatialReference;e*=3,n*=3;for(let v=0;v<r;++v){const f=t[e],h=t[e+1],$=t[e+2],S=o.getElevation(f,h,$,m,"scene")??0;l+=S,i[n]=f,i[n+1]=h,i[n+2]=p==null?$+S+d:S+d,e+=3,n+=3}return l/r}function Sn(t){const e=t.meterUnitOffset,i=t.featureExpressionInfoContext;return e!==0||i!=null}function xn(t,e,i,n,r,o,c,s){const l=s.calculateOffsetRenderUnits(c),d=s.featureExpressionInfoContext;e*=3,n*=3;for(let p=0;p<r;++p){const m=t[e],v=t[e+1],f=t[e+2];i[n]=m,i[n+1]=v,i[n+2]=d==null?f+l:l,e+=3,n+=3}return 0}class bn{constructor(){this.verticalDistanceToGround=0,this.sampledElevation=0,this.z=0}}const yn={"absolute-height":{applyElevationAlignmentBuffer:xn,requiresAlignment:Sn},"on-the-ground":{applyElevationAlignmentBuffer:mn,requiresAlignment:()=>!0},"relative-to-ground":{applyElevationAlignmentBuffer:gn,requiresAlignment:()=>!0},"relative-to-scene":{applyElevationAlignmentBuffer:vn,requiresAlignment:()=>!0}},$n=zt(),ee=new bn,he=A();let Tn=class{constructor(e,i){this.vec3=e,this.id=i}};function ut(t,e,i,n){return new Tn(Xt(t,e,i),n)}const I={dash:[4,3],dot:[1,3],"long-dash":[8,3],"short-dash":[4,1],"short-dot":[1,1]},wn={dash:I.dash,"dash-dot":[...I.dash,...I.dot],dot:I.dot,"long-dash":I["long-dash"],"long-dash-dot":[...I["long-dash"],...I.dot],"long-dash-dot-dot":[...I["long-dash"],...I.dot,...I.dot],none:null,"short-dash":I["short-dash"],"short-dash-dot":[...I["short-dash"],...I["short-dot"]],"short-dash-dot-dot":[...I["short-dash"],...I["short-dot"],...I["short-dot"]],"short-dot":I["short-dot"],solid:null},Dn=8;let Pn=class{constructor(e,i,n){this.image=e,this.width=i,this.length=n,this.uuid=Zt()}};function Wt(t){return t!=null&&"image"in t}function zn(t,e){return t==null?t:{pattern:t.slice(),pixelRatio:e}}function Ha(t){return{pattern:[t,t],pixelRatio:2}}function Ga(t){switch(t?.type){case"style":return Ln(t.style);case"image":return new Pn(t.image,t.width,t.length);case void 0:case null:return null}return null}function Ln(t){return t!=null?zn(wn[t],Dn):null}const ft=8;function _n(t,e){const{vertex:i}=t;i.uniforms.add(new j("intrinsicWidth",o=>o.width));const{hasScreenSizePerspective:n,spherical:r}=e;n?(t.include(on,e),sn(i),Ft(i,e),i.uniforms.add(new mi("inverseViewMatrix",(o,c)=>Qt(ht,Kt(ht,c.camera.viewMatrix,o.origin)))),i.code.add(a`
      float applyLineSizeScreenSizePerspective(float size, vec3 pos) {
        vec3 worldPos = (inverseViewMatrix * vec4(pos, 1)).xyz;
        vec3 groundUp = ${r?a`normalize(worldPos + localOrigin)`:a`vec3(0.0, 0.0, 1.0)`};
        float absCosAngle = abs(dot(groundUp, normalize(worldPos - cameraPosition)));

        return screenSizePerspectiveScaleFloat(size, absCosAngle, length(pos), screenSizePerspective);
      }
    `)):i.code.add(a`float applyLineSizeScreenSizePerspective(float size, vec3 pos) {
return size;
}`),e.hasVVSize?(i.uniforms.add(new ne("vvSizeMinSize",o=>o.vvSize.minSize),new ne("vvSizeMaxSize",o=>o.vvSize.maxSize),new ne("vvSizeOffset",o=>o.vvSize.offset),new ne("vvSizeFactor",o=>o.vvSize.factor),new ne("vvSizeFallback",o=>o.vvSize.fallback)),i.code.add(a`
    float getSize(${F(n,"vec3 pos")}) {
      float value = ${i.inputs.get("sizeFeatureAttribute")};
      float size = isnan(value)
        ? vvSizeFallback.x
        : intrinsicWidth * clamp(vvSizeOffset + value * vvSizeFactor, vvSizeMinSize, vvSizeMaxSize).x;

      return ${F(n,"applyLineSizeScreenSizePerspective(size, pos)","size")};
    }
    `)):i.code.add(a`
    float getSize(${F(n,"vec3 pos")}) {
      float fullSize = intrinsicWidth * ${i.inputs.get("size")};
      return ${F(n,"applyLineSizeScreenSizePerspective(fullSize, pos)","fullSize")};
    }
    `),e.hasVVOpacity?(i.constants.add("vvOpacityNumber","int",8),i.uniforms.add(new Qe("vvOpacityValues",ft,o=>o.vvOpacity.values),new Qe("vvOpacityOpacities",ft,o=>o.vvOpacity.opacityValues),new j("vvOpacityFallback",o=>o.vvOpacity.fallback,{supportsNaN:!0})),i.code.add(a`
    float interpolateOpacity(float value) {
      if (value <= vvOpacityValues[0]) {
        return vvOpacityOpacities[0];
      }

      for (int i = 1; i < vvOpacityNumber; ++i) {
        if (vvOpacityValues[i] >= value) {
          float f = (value - vvOpacityValues[i-1]) / (vvOpacityValues[i] - vvOpacityValues[i-1]);
          return mix(vvOpacityOpacities[i-1], vvOpacityOpacities[i], f);
        }
      }

      return vvOpacityOpacities[vvOpacityNumber - 1];
    }

    vec4 applyOpacity(vec4 color) {
      float value = ${i.inputs.get("opacityFeatureAttribute")};
      if (isnan(value)) {
        // If there is a color vv then it will already have taken care of applying the fallback
        return ${F(e.hasVVColor,"color","vec4(color.rgb, vvOpacityFallback)")};
      }

      return vec4(color.rgb, interpolateOpacity(value));
    }
    `)):i.code.add(a`vec4 applyOpacity(vec4 color) {
return color;
}`),e.hasVVColor?(t.include(gi,e),i.code.add(a`
    vec4 getColor() {
      vec4 color = interpolateVVColor(${i.inputs.get("colorFeatureAttribute")});

      // if we encounter NaN in the color it means the color is in the fallback case where the symbol color
      // is not defined and there is no valid color visual variable override. In this case just return a fully
      // transparent color
      if (isnan(color.r)) {
        return vec4(0);
      }

      return applyOpacity(color);
    }
    `)):i.code.add(a`
    vec4 getColor() {
      return applyOpacity(${i.inputs.get("color")});
    }
    `)}const ht=zt();function Cn(t){t.vertex.code.add("#define noPerspectiveWrite(x, w) (x * w)")}function We(t){t.fragment.code.add("#define noPerspectiveRead(x) (x * gl_FragCoord.w)")}function On(t){return t.pattern.map(e=>Math.round(e*t.pixelRatio))}function En(t){if(t==null)return 1;const e=On(t);return Math.floor(e.reduce((i,n)=>i+n))}function Fn(t){return t==null?ei:t.length===4?t:ti(An,t[0],t[1],t[2],1)}const An=ii();function Rn(t,e){if(!e.stippleEnabled)return void t.fragment.code.add(a`float getStippleAlpha(float lineWidth) { return 1.0; }
void discardByStippleAlpha(float stippleAlpha, float threshold) {}
vec4 blendStipple(vec4 color, float stippleAlpha) { return color; }`);const i=!(e.draped&&e.stipplePreferContinuous),{vertex:n,fragment:r}=t;e.draped||(Ft(n,e),n.uniforms.add(new xe("worldToScreenPerDistanceRatio",({camera:o})=>1/o.perScreenPixelRatio)).code.add(a`float computeWorldToScreenRatio(vec3 segmentCenter) {
float segmentDistanceToCamera = length(segmentCenter - cameraPosition);
return worldToScreenPerDistanceRatio / segmentDistanceToCamera;
}`)),t.varyings.add("vStippleDistance","float"),t.varyings.add("vStippleDistanceLimits","vec2"),t.varyings.add("vStipplePatternStretch","float"),n.code.add(a`
    float discretizeWorldToScreenRatio(float worldToScreenRatio) {
      float step = ${a.float(In)};

      float discreteWorldToScreenRatio = log(worldToScreenRatio);
      discreteWorldToScreenRatio = ceil(discreteWorldToScreenRatio / step) * step;
      discreteWorldToScreenRatio = exp(discreteWorldToScreenRatio);
      return discreteWorldToScreenRatio;
    }
  `),je(n),n.code.add(a`
    vec2 computeStippleDistanceLimits(float startPseudoScreen, float segmentLengthPseudoScreen, float segmentLengthScreen, float patternLength) {

      // First check if the segment is long enough to support fully screen space patterns.
      // Force sparse mode for segments that are very large in screen space even if it is not allowed,
      // to avoid imprecision from calculating with large floats.
      if (segmentLengthPseudoScreen >= ${i?"patternLength":"1e4"}) {
        // Round the screen length to get an integer number of pattern repetitions (minimum 1).
        float repetitions = segmentLengthScreen / (patternLength * pixelRatio);
        float flooredRepetitions = max(1.0, floor(repetitions + 0.5));
        float segmentLengthScreenRounded = flooredRepetitions * patternLength;

        float stretch = repetitions / flooredRepetitions;

        // We need to impose a lower bound on the stretch factor to prevent the dots from merging together when there is only 1 repetition.
        // 0.75 is the lowest possible stretch value for flooredRepetitions > 1, so it makes sense as lower bound.
        vStipplePatternStretch = max(0.75, stretch);

        return vec2(0.0, segmentLengthScreenRounded);
      }
      return vec2(startPseudoScreen, startPseudoScreen + segmentLengthPseudoScreen);
    }
  `),r.uniforms.add(new Zi("stipplePatternTexture",o=>o.stippleTexture),new j("stipplePatternPixelSizeInv",o=>1/Vt(o))),e.stippleOffColorEnabled&&r.uniforms.add(new Ie("stippleOffColor",o=>Fn(o.stippleOffColor))),t.include(We),e.worldSizedImagePattern?(t.varyings.add("vStippleV","float"),t.fragment.include(ln),r.code.add(a`vec4 getStippleColor(out bool isClamped) {
vec2 aaCorrectedLimits = vStippleDistanceLimits + vec2(1.0, -1.0) / gl_FragCoord.w;
isClamped = vStippleDistance < aaCorrectedLimits.x || vStippleDistance > aaCorrectedLimits.y;
float u = vStippleDistance * stipplePatternPixelSizeInv;
float v = vStippleV == -1.0 ? 0.5 : vStippleV;
return texture(stipplePatternTexture, vec2(u, v));
}
vec4 getStippleColor() {
bool ignored;
return getStippleColor(ignored);
}
float getStippleSDF() {
vec4 color = getStippleColor();
return color.a == 0.0 ? -0.5 : 0.5;
}
float getStippleAlpha(float lineWidth) {
return getStippleColor().a;
}
vec4 blendStipple(vec4 color, float stippleAlpha) {
vec4 stippleColor = getStippleColor();
int mixMode  = 1;
vec3 col = mixExternalColor(color.rgb, vec3(1.0), stippleColor.rgb, mixMode);
float opacity = mixExternalOpacity(color.a, 1.0, stippleColor.a, mixMode);
return vec4(col, opacity);
}`)):r.code.add(a`
    float getStippleSDF(out bool isClamped) {
      float stippleDistanceClamped = noPerspectiveRead(clamp(vStippleDistance, vStippleDistanceLimits.x, vStippleDistanceLimits.y));
      float lineSizeInv = noPerspectiveRead(vLineSizeInv);

      vec2 aaCorrectedLimits = vStippleDistanceLimits + vec2(1.0, -1.0) / gl_FragCoord.w;
      isClamped = vStippleDistance < aaCorrectedLimits.x || vStippleDistance > aaCorrectedLimits.y;

      float u = stippleDistanceClamped * stipplePatternPixelSizeInv * lineSizeInv;
      u = fract(u);

      float sdf = texture(stipplePatternTexture, vec2(u, 0.5)).r;

      return (sdf - 0.5) * vStipplePatternStretch + 0.5;
    }

    float getStippleSDF() {
      bool ignored;
      return getStippleSDF(ignored);
    }

    float getStippleAlpha(float lineWidth) {
      bool isClamped;
      float stippleSDF = getStippleSDF(isClamped);
      float antiAliasedResult = clamp(stippleSDF * lineWidth + 0.5, 0.0, 1.0);
      return isClamped ? floor(antiAliasedResult + 0.5) : antiAliasedResult;
    }

    vec4 blendStipple(vec4 color, float stippleAlpha) {
      return ${e.stippleOffColorEnabled?"mix(color, stippleOffColor, stippleAlpha)":"vec4(color.rgb, color.a * stippleAlpha)"};
    }
  `),r.code.add(a`
    void discardByStippleAlpha(float stippleAlpha, float threshold) {
     ${F(!e.stippleOffColorEnabled,"if (stippleAlpha < threshold) { discard; }")}
    }
  `)}function Vt(t){const e=t.stipplePattern;return Wt(e)?e.length:e?En(e)/e.pixelRatio:1}const In=.4,Nt=64,Wn=Nt/2,Vn=Wn/5,Nn=Nt/Vn,qa=.25;function Mn(t,e){const i=t.vertex,n=e.hasScreenSizePerspective;je(i),i.uniforms.get("markerScale")==null&&i.constants.add("markerScale","float",1),i.constants.add("markerSizePerLineWidth","float",Nn).code.add(a`
  float getLineWidth(${F(n,"vec3 pos")}) {
     return max(getSize(${F(n,"pos")}), 1.0) * pixelRatio;
  }

  float getScreenMarkerSize(float lineWidth) {
    return markerScale * markerSizePerLineWidth * lineWidth;
  }
  `),e.space===2&&(i.constants.add("maxSegmentLengthFraction","float",.45),i.uniforms.add(new xe("perRenderPixelRatio",r=>r.camera.perRenderPixelRatio)),i.code.add(a`
  bool areWorldMarkersHidden(vec3 pos, vec3 other) {
    vec3 midPoint = mix(pos, other, 0.5);
    float distanceToCamera = length(midPoint);
    float screenToWorldRatio = perRenderPixelRatio * distanceToCamera * 0.5;
    float worldMarkerSize = getScreenMarkerSize(getLineWidth(${F(n,"pos")})) * screenToWorldRatio;
    float segmentLen = length(pos - other);
    return worldMarkerSize > maxSegmentLengthFraction * segmentLen;
  }

  float getWorldMarkerSize(vec3 pos) {
    float distanceToCamera = length(pos);
    float screenToWorldRatio = perRenderPixelRatio * distanceToCamera * 0.5;
    return getScreenMarkerSize(getLineWidth(${F(n,"pos")})) * screenToWorldRatio;
  }
  `))}const jn=a`vec4(0.0, 0.0, 2.0, 1.0)`,kn=le(1),Bn=le(1);function Jn(t,e){const{hasAnimation:i,animation:n}=e;if(!i)return;const{attributes:r,varyings:o,vertex:c,fragment:s}=t,l=e.timeStampsExpr??"timeStamps";e.timeStampsExpr==null&&r.add("timeStamps","vec4"),o.add("vTimeStamp","float"),o.add("vFirstTime","float"),o.add("vLastTime","float"),o.add("vTransitionType","float"),c.main.add(a`
    vec4 animatedLineTimeStamps = ${l};
    vTimeStamp = animatedLineTimeStamps.x;
    vFirstTime = animatedLineTimeStamps.y;
    vLastTime = animatedLineTimeStamps.z;
    vTransitionType = animatedLineTimeStamps.w;
  `),n===3&&s.constants.add("decayRate","float",2.3),s.code.add(a`
    float getTrailOpacity(float x) {
      if (x < 0.0) {
        return 0.0;
      }

      ${Un(n)}
    }`),s.uniforms.add(new j("timeElapsed",d=>d.timeElapsed),new j("trailLength",d=>d.trailLength),new j("speed",d=>d.animationSpeed),new cn("startEndTime",d=>Ae(Hn,d.startTime,d.endTime))),s.constants.add("fadeInTime","float",Bn),s.constants.add("fadeOutTime","float",kn),s.constants.add("incomingTransition","int",0),s.constants.add("outgoingTransition","int",2),s.code.add(a`float fadeIn(float x) {
return smoothstep(0.0, fadeInTime, x);
}
float fadeOut(float x) {
return isinf(fadeOutTime) ? 1.0 : smoothstep(fadeOutTime, 0.0, x);
}
void updateAlphaIf(inout float alpha, bool condition, float newAlpha) {
alpha = condition ? min(alpha, newAlpha) : alpha;
}
vec4 animate(vec4 color) {
float startTime = startEndTime[0];
float endTime = startEndTime[1];
float totalTime = vLastTime - vFirstTime;
float actualFadeOutTime = min(fadeOutTime * speed, trailLength);
float longStreamlineThreshold = (fadeInTime + 1.0) * speed + actualFadeOutTime;
bool longStreamline = totalTime > longStreamlineThreshold;
float totalTimeWithFadeOut = longStreamline && actualFadeOutTime != trailLength ? totalTime : totalTime + actualFadeOutTime;
float fadeOutStartTime = longStreamline ? totalTime - actualFadeOutTime : totalTime;
float originTime =  -vFirstTime;
float actualEndTime = int(vTransitionType) == outgoingTransition ? min(endTime, startTime + vLastTime / speed) : endTime;
vec4 animatedColor = color;
if (speed == 0.0) {
float alpha = getTrailOpacity((totalTimeWithFadeOut - (vTimeStamp - vFirstTime)) / trailLength);
updateAlphaIf(alpha, !isinf(actualEndTime), fadeOut(timeElapsed - actualEndTime));
updateAlphaIf(alpha, true, fadeIn(timeElapsed - startTime));
animatedColor.a *= alpha;
return animatedColor;
}
float relativeStartTime = mod(startTime, totalTimeWithFadeOut);
float shiftedTimeElapsed = timeElapsed - relativeStartTime + originTime;
float headRelativeToFirst = mod(shiftedTimeElapsed * speed, totalTimeWithFadeOut);
float vRelativeToHead = headRelativeToFirst - originTime - vTimeStamp;
float vAbsoluteTime = timeElapsed - vRelativeToHead / speed;
if (startTime > timeElapsed) {
return vec4(0.0);
}
float alpha = getTrailOpacity(vRelativeToHead / trailLength);
updateAlphaIf(alpha, true, fadeIn(timeElapsed - startTime));
updateAlphaIf(alpha, !isinf(actualEndTime), fadeOut(timeElapsed - actualEndTime));
updateAlphaIf(alpha, int(vTransitionType) != incomingTransition, step(startTime, vAbsoluteTime));
updateAlphaIf(alpha, headRelativeToFirst > fadeOutStartTime, fadeOut((headRelativeToFirst - fadeOutStartTime) / speed));
alpha *= fadeIn(vTimeStamp - vFirstTime);
animatedColor.a *= alpha;
return animatedColor;
}`)}function Un(t){switch(t){case 2:return"return x >= 0.0 && x <= 1.0 ? 1.0 : 0.0;";case 3:return`float cutOff = exp(-decayRate);
        return (exp(-decayRate * x) - cutOff) / (1.0 - cutOff);`;default:return"return 1.0;"}}const Hn=Me();function Gn(t){switch(t.elementType){case"float":switch(t.elementCount){case 1:return a`float`;case 2:return a`vec2`;case 3:return a`vec3`;case 4:return a`vec4`;case 9:return a`mat3`;default:t.elementCount}break;case"int":switch(t.elementCount){case 1:return a`int`;case 2:return a`ivec2`;case 3:return a`ivec3`;case 4:return a`ivec4`;case 9:throw new Error("Invalid element count 9 for type int");default:t.elementCount}break;case"uint":switch(t.elementCount){case 1:return a`uint`;case 2:return a`uvec2`;case 3:return a`uvec3`;case 4:return a`uvec4`;case 9:throw new Error("Invalid element count 9 for type uint");default:t.elementCount}break;default:t.elementType}throw new Error("unsupported field")}const Mt=new xe("constNaN",()=>NaN,{supportsNaN:!0});let ke=class extends Qi{constructor(e){super(),this.supportNaN=e}};function qn(t,e){const i=e?.supportNaN;i&&(t.uniforms.add(Mt),t.code.add(a`bool bitsEncodeFloat16NaN(highp uint bits) {
const highp uint nanExponent = 0x00007c00u;
highp uint exponent = bits & nanExponent;
highp uint mantissa = bits & 0x000003ffu;
return exponent == nanExponent && mantissa != 0u;
}`)),t.code.add(a`
    mediump float unpackHalf1x16(highp uint bits) {
      ${F(i,a`
        if (bitsEncodeFloat16NaN(bits)) {
          return constNaN;
        }`)}
      return unpackHalf2x16(bits).x;
    }`),t.code.add(a`
    mediump vec2 unpackHalf2x16NaNSupport(highp uint bits) {
      vec2 result = unpackHalf2x16(bits);
      ${F(i,a`
        if (bitsEncodeFloat16NaN(bits)) {
          result.x = constNaN;
        }
        if (bitsEncodeFloat16NaN(bits >> ${a.uint(q[2])})) {
          result.y = constNaN;
        }
        `)}
      return result;
    }`)}function Yn(t,e){const i=e?.supportNaN;i&&(t.uniforms.add(Mt),t.code.add(a`bool bitsEncodeFloat32NaN(highp uint bits) {
const highp uint nanExponent = 0x7f800000u;
highp uint exponent = bits & nanExponent;
highp uint mantissa = bits & 0x007fffffu;
return exponent == nanExponent && mantissa != 0u;
}`)),t.code.add(a`
    highp float unpackFloat1x32(highp uint bits) {
      ${F(i,a`
        if (bitsEncodeFloat32NaN(bits)) {
          return constNaN;
        }`)}
      return uintBitsToFloat(bits);
    }`)}function Xn(t){t.code.add(a`mediump int unpackInt1x16(highp uint bits) {
highp uint signExtendedBits = (bits & 0x8000u) != 0u ? (bits | 0xffff0000u) : bits;
return int(signExtendedBits);
}`)}function Zn(t,e){const{fieldType:i}=t;return`${(0,Kn[i])(ea(t,e))}`}function ve(t,e){const i=[];for(const n of t){const r=a`unpackFloat1x32(${n})`;i.push(r)}return i.join(e)}L([_()],ke.prototype,"supportNaN",void 0);const jt=t=>a`${t[0]}`,mt=t=>{const e=t[0],i=a`uvec4(${a.uint(q[0])}, ${a.uint(q[1])}, ${a.uint(q[2])}, ${a.uint(q[3])})`,n=a`uvec4(${a.hexuint(kt[1])})`;return a`((uvec4(${e}) >> ${i}) & ${n})`},Qn=t=>a`(float(${jt(t)})/${a.float(Lt)})`,gt=t=>a`unpackFloat1x32(${t[0]})`,vt=t=>a`vec4(${ve(t,", ")})`,Kn={u8:jt,u32:t=>a`${t[0]}`,vec4u32:t=>a`uvec4(${t.join(", ")})`,unorm8:Qn,vec4unorm8:t=>a`(vec4(${mt(t)})/${a.float(Lt)})`,snorm16:t=>a`unpackSnorm2x16(${t[0]}).x`,vec2snorm16:t=>a`unpackSnorm2x16(${t[0]})`,f16:rt?t=>a`unpackHalf1x16(${t[0]})`:gt,vec4f16:rt?t=>a`vec4(unpackHalf2x16NaNSupport(${t[0]}), unpackHalf2x16NaNSupport(${t[1]}))`:vt,f32:gt,vec4u8:mt,vec2f32:t=>a`vec2(${ve(t,", ")})`,vec3f32:t=>a`vec3(${ve(t,", ")})`,vec4f32:vt,mat3f32:t=>a`mat3(${ve(t,`,
`)})`};function ea(t,e){const{byteOffset:i,byteSize:n}=t,r=e.channelByteStride,o=e.byteStride,c=Math.ceil(n/De),s=ta[e.channels],l=new Array;for(let d=0;d<c;++d){const p=d*De,m=i+p,v=n-p,f=Math.min(v,De);let h=0;const $=new Array;for(;h<f;){const S=m+h,u=Math.floor(S/o),x=S%o,O=Math.floor(x/r),b=x%r,T=r-b,w=f-h,D=Math.min(T,w),W=a`texel${a.int(u)}${s[O]}`,z=D===4?"":a` & ${a.hexuint(kt[D])}`,g=b===0?"":a` >> ${a.uint(q[b])}`,y=a`((${W}${g})${z})`,P=h===0?"":a` << ${a.uint(q[h])}`,E=a`(${y})${P}`;$.push(E),h+=D}l.push(a`(${$.join(" | ")})`)}return l}const De=4,q=[0,8,16,24],kt=[0,255,65535,16777215,4294967295],ta={1:[a``],2:[a`.x`,a`.y`],4:[a`.x`,a`.y`,a`.z`,a`.w`]},ia=new ke(!0),na=new ke(!1);class aa{constructor(e,i){this._shader=e,this._namespace=i,this._used=new Map}getItemSuffix(e){return e===0?"":`${e<0?"Minus":"Plus"}${Math.abs(e)}`}getItemDataName(e=0){return`${this._namespace}ItemData${this.getItemSuffix(e)}`}getStructName(e=0){return`${this._namespace}TextureBackedBufferItemData${this.getItemSuffix(e)}`}getFetchName(e=0){return`${this._namespace}fetchTextureBackedBufferItemData${this.getItemSuffix(e)}`}getStrideName(){return`${this._namespace}tbbStride`}getTextureAttribute(e,i=0){const n=this._used,r=this.getItemDataName(i);let o=n.get(i);return o==null&&(o=new Set,n.set(i,o)),o.add(e),a`${r}.${e}`}generateVertexCode(e){const{_shader:i,_used:n}=this,r=new pt(i.vertex);for(const o of n.keys())this._generateFetchFunction(o,r,e);return r.generateSource()}generateVertexMainCode(e){const{_shader:i,_used:n}=this,r=new pt(i.vertex);for(const o of n.keys())this._generateFetchFunctionCall(o,r,e);return r.generateSource()}_generateFetchFunctionCall(e,i,n){const{itemIndexExpression:r}=n,o=this._used.get(e);if(o==null||o.size===0)return;const c=this.getItemDataName(e),s=this.getFetchName(e);i.add(a`${c} = ${s}(${r});`)}_getIndexOffset(e=0){return e===0?a``:a`${e<0?"-":"+"}${a.uint(Math.abs(e))}`}_generateFetchFunction(e,i,n){const{bufferUniform:r,layout:o}=n,{texelFormatInfo:c}=o,s=this._used.get(e);if(s==null||s.size===0)return;const l=this.getStrideName(),d=this.getStructName(e),p=this.getItemDataName(e),m=this.getFetchName(e),v=this._getIndexOffset(e),f=new Array;for(const u of o.fields.values())s.has(u.name)&&f.push(u);if(f.length===0)return;const h=[];for(let u=0;u<o.texelStride;++u)h.push(!1);for(const u of f)for(let x=0;x<u.numTexels;++x)h[u.startTexel+x]=!0;i.add(a`
  struct ${d} {`);for(const u of f)i.add(a`\t${Gn(u)} ${u.name};`);i.add(a`};`),i.add(a`\n${d} ${m}( highp uint baseIndex ) {
    ${d} itemData;
    highp uint index = (baseIndex${v}) * ${l};
    highp uint rowWidth = uint(textureSize(${r.name}, 0).x);
    int coordX = int(index % rowWidth);
    int coordY = int(index / rowWidth);\n`);const $=oa[c.channels],S=sa[c.channels];for(let u=0;u<h.length;++u)h[u]!==!1&&i.add(a`highp ${$} texel${a.int(u)} = texelFetch(${r.name}, ivec2(coordX + ${a.int(u)}, coordY), 0)${S};`);for(const u of f)i.add(a`itemData.${u.name} = ${Zn(u,c)};`);i.add(a`return itemData;\n}`),i.add(a`${d} ${p};`)}}class ra{constructor(e){this._parameters=e,this.moduleId=_t(),this.namespace=`_tbb_${this.moduleId}_`}createBuilder(e,i){let n=null;const r=o=>{n=this._buildTextureBackedBufferShaderCode(o)};return i?e.include(r,i):e.include(r),H(n!=null,"Valid builder expected."),n}_buildTextureBackedBufferShaderCode(e){const{namespace:i,_parameters:n}=this,{bufferUniform:r,layout:o}=n,c=n.enableNaNSupport?ia:na,{vertex:s}=e,l=new aa(e,i);s.include(Yn,c),s.include(qn,c),s.include(Xn);const d=l.getStrideName(),p=l.getStructName(),m=l.getFetchName(),v=l.getItemDataName();for(const f of[d,p,m,v])H(f.length<1024,"Identifiers do not have a valid length");return s.constants.add(d,"uint",o.texelStride),s.uniforms.add(r),s.code.add(()=>l.generateVertexCode(n)),s.main.add(()=>l.generateVertexMainCode(n)),l}}const oa={1:a`uint`,2:a`uvec2`,4:a`uvec4`},sa={1:a`.x`,2:a`.xy`,4:""};class la extends Ki{constructor(e,i){super(e,"usampler2D",2,(n,r,o)=>n.bindTexture(e,i(r,o)))}}function Bt(t){return an().u32("textureElementIndex",{integer:!0}).vec2f16("lineParameters")}const ca=[{type:"vec3f32",name:"position"},{type:"f32",name:"u0"}];function Jt(t){const e=[...ca];return t.hasVVColor?e.push({type:"f32",name:"colorFeatureAttribute"}):e.push({type:"vec4unorm8",name:"color"}),t.hasVVSize?e.push({type:"f32",name:"sizeFeatureAttribute"}):e.push({type:"f32",name:"size"}),t.hasVVOpacity&&e.push({type:"f32",name:"opacityFeatureAttribute"}),At()&&e.push({type:"vec4unorm8",name:"olidColor"}),t.hasAnimation&&e.push({type:"vec4f16",name:"timeStamps"}),new fn(e)}const da=new la("componentTextureBuffer",t=>t.textureBuffer);function pa(t){return new ra({layout:Jt(t),itemIndexExpression:"textureElementIndex",bufferUniform:da})}const Be=1;function Ut(t){const e=new un,{attributes:i,varyings:n,vertex:r,fragment:o}=e,{applyMarkerOffset:c,draped:s,output:l,capType:d,stippleEnabled:p,falloffEnabled:m,roundJoins:v,wireframe:f,innerColorEnabled:h,hasAnimation:$,hasScreenSizePerspective:S,worldSizedImagePattern:u}=t,x=pa(t).createBuilder(e,t);r.inputs.add("position",()=>x.getTextureAttribute("position")),t.hasVVSize?r.inputs.add("sizeFeatureAttribute",()=>x.getTextureAttribute("sizeFeatureAttribute")):r.inputs.add("size",()=>x.getTextureAttribute("size")),t.hasVVOpacity&&r.inputs.add("opacityFeatureAttribute",()=>x.getTextureAttribute("opacityFeatureAttribute")),t.hasVVColor?r.inputs.add("colorFeatureAttribute",()=>x.getTextureAttribute("colorFeatureAttribute")):r.inputs.add("color",()=>x.getTextureAttribute("color")),o.include(dn),e.include(_n,t),e.include(Rn,t);const O={animation:t.animation,hasAnimation:t.hasAnimation,timeStampsExpr:x.getTextureAttribute("timeStamps")};e.include(Jn,O),l===11?(e.varyings.add("objectAndLayerIdColorVarying","vec4"),r.code.add(a`
      vec4 getOlidColor() {
        return ${x.getTextureAttribute("olidColor")};
      }

      void forwardObjectAndLayerIdColor() {
        objectAndLayerIdColorVarying = getOlidColor();
      }
    `),o.code.add(a`void outputObjectAndLayerIdColor() {
fragColor = objectAndLayerIdColorVarying;
}`)):(r.code.add(a`void forwardObjectAndLayerIdColor() {}`),o.code.add(a`void outputObjectAndLayerIdColor() {}`));const b=c&&!s;b&&(r.uniforms.add(new j("markerScale",g=>g.markerScale)),e.include(Mn,{space:2,hasScreenSizePerspective:S})),vi(r,t),r.uniforms.add(new Si("inverseProjectionMatrix",g=>g.camera.inverseProjectionMatrix),new xi("nearFar",g=>g.camera.nearFar),new j("miterLimit",g=>g.join!=="miter"?0:g.miterLimit),new pn("viewport",g=>g.camera.fullViewport)),r.constants.add("LARGE_HALF_FLOAT","float",65500),r.constants.add("EPS","float",.001),r.constants.add("NUM_JOIN_SUBDIVISIONS","float",t.numJoinSubdivisions),i.add("textureElementIndex","uint"),i.add("lineParameters","vec2"),n.add("vColor","vec4"),n.add("vpos","vec3",{invariant:!0}),n.add("vLineDistance","float"),n.add("vLineWidth","float"),p||(n.add("vIsInsideJoin","int"),n.add("vStretchFactor","float"),n.add("vJoinCenterLineSDFs","vec2"),n.add("vSubdivisionFactor","float"));const T=p;T&&n.add("vLineSizeInv","float");const w=d===2,D=p&&w,W=m||D;W&&n.add("vLineDistanceNorm","float"),w&&(n.add("vSegmentSDF","float"),n.add("vReverseSegmentSDF","float")),r.code.add(a`vec3 perpendicular(vec3 v) {
return vec3(v.y, -v.x, 0.0);
}
float interp(float ncp, vec4 a, vec4 b) {
return (-ncp - a.z) / (b.z - a.z);
}
vec3 rotateZ(vec3 v, float a) {
float s = sin(a);
float c = cos(a);
mat2 m = mat2(c, -s, s, c);
return vec3(m * v.xy, v.z);
}`),r.code.add(a`vec4 projectAndScale(vec4 pos) {
vec4 posNdc = proj * pos;
posNdc.xy *= viewport.zw / posNdc.w;
posNdc.z /= posNdc.w;
return posNdc;
}`),r.code.add(a`void clip(
inout vec4 pos,
inout vec4 prev,
inout vec4 next,
bool isStartVertex
) {
float vnp = nearFar[0] * 0.99;
if (pos.z > -nearFar[0]) {
if (!isStartVertex) {
if (prev.z < -nearFar[0]) {
pos = mix(prev, pos, interp(vnp, prev, pos));
next = pos;
} else {
pos = vec4(0.0, 0.0, 0.0, 1.0);
}
} else {
if (next.z < -nearFar[0]) {
pos = mix(pos, next, interp(vnp, pos, next));
prev = pos;
} else {
pos = vec4(0.0, 0.0, 0.0, 1.0);
}
}
} else {
if (prev.z > -nearFar[0]) {
prev = mix(pos, prev, interp(vnp, pos, prev));
}
if (next.z > -nearFar[0]) {
next = mix(next, pos, interp(vnp, next, pos));
}
}
}`),je(r),r.constants.add("aaWidth","float",p?0:1),r.main.add(a`
    vec3 position = ${x.getTextureAttribute("position")};
    float u0 = ${x.getTextureAttribute("u0")};

    // unpack values from vertex type
    bool isStartVertex = abs(abs(lineParameters.y) - 3.0) == 1.0;
    vec3 prevPosition = ${x.getTextureAttribute("position",-1)};
    vec3 nextPosition = ${x.getTextureAttribute("position",1)};

    float coverage = 1.0;

    // Check for special value of lineParameters.y which is used by the Renderer when graphics are removed before the
    // VBO is recompacted. If this is the case, then we just project outside of clip space.
    if (lineParameters.y == 0.0) {
      gl_Position = ${jn};
    }
    else {
      vec4 pos  = view * vec4(position, 1.0);
      vec4 prev = view * vec4(prevPosition, 1.0);
      vec4 next = view * vec4(nextPosition, 1.0);

      bool isJoin = abs(lineParameters.y) < 3.0;
  `),b&&r.main.add(a`vec4 other = isStartVertex ? next : prev;
bool markersHidden = areWorldMarkersHidden(pos.xyz, other.xyz);
if (!isJoin && !markersHidden) {
pos.xyz += normalize(other.xyz - pos.xyz) * getWorldMarkerSize(pos.xyz) * 0.5;
}`),e.include(Cn),r.main.add(a`
      clip(pos, prev, next, isStartVertex);

      vec3 clippedPos = pos.xyz;
      vec3 clippedCenter = mix(pos.xyz, isStartVertex ? next.xyz : prev.xyz, 0.5);

      pos = projectAndScale(pos);
      next = projectAndScale(next);
      prev = projectAndScale(prev);

      vec3 left = (pos.xyz - prev.xyz);
      vec3 right = (next.xyz - pos.xyz);

      float leftLen = length(left);
      float rightLen = length(right);

      float lineSize = getSize(${F(S,"clippedPos")});
      ${F(p&&S,"float patternLineSize = getSize(clippedCenter);")}
      ${F(p&&!S,"float patternLineSize = lineSize;")}

      ${F(u,a`
          lineSize += aaWidth;
          float lineWidth = lineSize * pixelRatio * worldToScreenRatio;
          if (lineWidth < 1.0) {
            coverage = lineWidth;
            lineWidth = 1.0;
          }
        `,a`
          if (lineSize < 1.0) {
            coverage = lineSize; // convert sub-pixel coverage to alpha
            lineSize = 1.0;
          }

          lineSize += aaWidth;
          float lineWidth = lineSize * pixelRatio;
        `)}

      vLineWidth = noPerspectiveWrite(lineWidth, pos.w);
      ${T?a`vLineSizeInv = noPerspectiveWrite(1.0 / lineSize, pos.w);`:""}
  `),(p||w)&&r.main.add(a`
      float isEndVertex = float(!isStartVertex);
      vec3 segmentOrigin = mix(pos.xyz, prev.xyz, isEndVertex);
      vec3 segment = mix(right, left, isEndVertex);
      ${w?a`vec3 segmentEnd = mix(next.xyz, pos.xyz, isEndVertex);`:""}
    `),r.main.add(a`left = (leftLen > EPS) ? left/leftLen : vec3(0.0, 0.0, 0.0);
right = (rightLen > EPS) ? right/rightLen : vec3(0.0, 0.0, 0.0);
vec3 segmentDirection = isStartVertex ? right : left;
vec3 capDisplacementDir = vec3(0.0, 0.0, 0.0);
vec3 joinDisplacementDir = vec3(0.0, 0.0, 0.0);
float displacementLen = lineWidth;
float miterDisplacementLen = lineWidth;
float innerDisplacementLen = lineWidth;`),p||r.main.add(a`vIsInsideJoin = 0;
vStretchFactor = 1.0;
vSubdivisionFactor = 0.0;
vJoinCenterLineSDFs = vec2(LARGE_HALF_FLOAT);`),r.main.add(a`float subdivisionFactor = 0.0;
bool isOutside = false;
if (isJoin) {
isOutside = (left.x * right.y - left.y * right.x) * lineParameters.y > 0.0;
vec3 joinDirection = normalize(left + right);
joinDisplacementDir = perpendicular(joinDirection);
if (leftLen > EPS && rightLen > EPS) {
float nDotSeg = dot(joinDisplacementDir, left);
displacementLen /= length(nDotSeg * left - joinDisplacementDir);
miterDisplacementLen = displacementLen;
innerDisplacementLen = min(displacementLen, min(leftLen, rightLen)/abs(nDotSeg));
if (!isOutside) {
displacementLen = innerDisplacementLen;
}
}
subdivisionFactor = lineParameters.x;`),p||r.main.add(a`if(subdivisionFactor > 0.0) {
vIsInsideJoin = 1;
}
vSubdivisionFactor = isOutside ? subdivisionFactor : 0.5;
if (miterDisplacementLen > miterLimit * lineWidth) {
vec2 leftScreenDir = left.xy;
vec2 rightScreenDir = right.xy;
float leftScreenLen = length(leftScreenDir);
float rightScreenLen = length(rightScreenDir);
if (leftScreenLen > EPS && rightScreenLen > EPS) {
leftScreenDir /= leftScreenLen;
rightScreenDir /= rightScreenLen;
float theta = acos(clamp(dot(leftScreenDir, rightScreenDir), -1.0, 1.0));
float subdividedTriangleHeight = (innerDisplacementLen + lineWidth) * cos(theta / (2.0 + 2.0 * NUM_JOIN_SUBDIVISIONS));
float bevelTriangleHeight = innerDisplacementLen + lineWidth * cos(theta * 0.5);
float triangleHeight = NUM_JOIN_SUBDIVISIONS > 0.0 ? subdividedTriangleHeight : bevelTriangleHeight;
vStretchFactor = noPerspectiveWrite(max(triangleHeight / (2.0 * lineWidth), 1.0), pos.w);
}
}`),r.main.add(a`if (isOutside && (displacementLen > miterLimit * lineWidth)) {`),v?r.main.add(a`
        vec3 startDir = leftLen < EPS ? right : left;
        startDir = perpendicular(startDir);

        vec3 endDir = rightLen < EPS ? left : right;
        endDir = perpendicular(endDir);

        float factor = ${p?a`min(1.0, subdivisionFactor * ((NUM_JOIN_SUBDIVISIONS + 1.0) / NUM_JOIN_SUBDIVISIONS))`:a`subdivisionFactor`};

        float rotationAngle = acos(clamp(dot(startDir.xy, endDir.xy), -1.0, 1.0));
        joinDisplacementDir = rotateZ(startDir, -sign(lineParameters.y) * factor * rotationAngle);
      `):r.main.add(a`
        vec3 startDir = perpendicular(leftLen < EPS ? right : left);
        vec3 endDir = perpendicular(rightLen < EPS ? left : right);

        ${F(p,a`joinDisplacementDir = (isStartVertex || subdivisionFactor > 0.0) ? endDir : startDir;`,a`joinDisplacementDir = mix(startDir, endDir, subdivisionFactor);`)}
  `);const z=d!==0;return r.main.add(a`
        displacementLen = lineWidth;
      }
    } else {
      // CAP handling ---------------------------------------------------
      joinDisplacementDir = isStartVertex ? right : left;
      joinDisplacementDir = perpendicular(joinDisplacementDir);

      ${z?a`capDisplacementDir = vec3((isStartVertex ? -right : left).xy, 0.0);`:""}
    }
  `),r.main.add(a`
    // Displacement (in pixels) caused by join/or cap
    vec2 dposXY = (joinDisplacementDir.xy * sign(lineParameters.y) + capDisplacementDir.xy) * displacementLen;

    /**
     * To prevent z-fighting between layers, we also adjust the z value.
     * We want to ensure that the orientation of the final triangles is the same, regardless of the line width.
     * To do so, the below formula projects the xy displacement onto the original segment direction
     * to find the z-offset necessary so the triangle orientation is independent of the width.
     */
    float dposZ = dot(dposXY, segmentDirection.xy) / dot(segmentDirection.xy, segmentDirection.xy) * segmentDirection.z;
    vec3 dpos = vec3(dposXY, dposZ);

    float lineDistNorm = noPerspectiveWrite(sign(lineParameters.y), pos.w);

    vLineDistance = lineWidth * lineDistNorm;
    ${W?a`vLineDistanceNorm = lineDistNorm;`:""}

    pos.xyz += dpos;
  `),p||r.main.add(a`if (isJoin) {
vec2 joinCenterToVertex = dposXY;
vec2 leftCenterlineDir = left.xy;
vec2 rightCenterlineDir = right.xy;
float leftCenterlineLen = length(leftCenterlineDir);
float rightCenterlineLen = length(rightCenterlineDir);
leftCenterlineDir = leftCenterlineLen > EPS ? leftCenterlineDir / leftCenterlineLen : vec2(1.0, 0.0);
rightCenterlineDir = rightCenterlineLen > EPS ? rightCenterlineDir / rightCenterlineLen : leftCenterlineDir;
vJoinCenterLineSDFs = noPerspectiveWrite(
vec2(
dot(vec2(rightCenterlineDir.y, -rightCenterlineDir.x), joinCenterToVertex),
dot(vec2(leftCenterlineDir.y, -leftCenterlineDir.x), joinCenterToVertex)
),
pos.w
);
}`),w&&r.main.add(a`vec2 segmentDir = normalize(segment.xy);
vSegmentSDF = noPerspectiveWrite((isJoin && isStartVertex) ? LARGE_HALF_FLOAT : (dot(pos.xy - segmentOrigin.xy, segmentDir)), pos.w);
vReverseSegmentSDF = noPerspectiveWrite((isJoin && !isStartVertex) ? LARGE_HALF_FLOAT : (dot(pos.xy - segmentEnd.xy, -segmentDir)), pos.w);`),p&&(s?r.uniforms.add(new xe("worldToScreenRatio",g=>1/g.screenToPCSRatio)):r.main.add(a`vec3 segmentCenter = mix((nextPosition + position) * 0.5, (position + prevPosition) * 0.5, isEndVertex);
float worldToScreenRatio = computeWorldToScreenRatio(segmentCenter);`),r.main.add(a`float segmentLengthScreenDouble = length(segment.xy);
float segmentLengthScreen = segmentLengthScreenDouble * 0.5;
float discreteWorldToScreenRatio = discretizeWorldToScreenRatio(worldToScreenRatio);
float segmentLengthRender = length(mix(nextPosition - position, position - prevPosition, isEndVertex));
vStipplePatternStretch = worldToScreenRatio / discreteWorldToScreenRatio;`),s?r.main.add(a`float segmentLengthPseudoScreen = segmentLengthScreen / pixelRatio * discreteWorldToScreenRatio / worldToScreenRatio;
float startPseudoScreen = u0 * discreteWorldToScreenRatio - mix(0.0, segmentLengthPseudoScreen, isEndVertex);`):r.main.add(a`float startPseudoScreen = mix(u0, u0 - segmentLengthRender, isEndVertex) * discreteWorldToScreenRatio;
float segmentLengthPseudoScreen = segmentLengthRender * discreteWorldToScreenRatio;`),r.uniforms.add(new j("stipplePatternPixelSize",g=>Vt(g))),r.main.add(a`
      float patternLength = patternLineSize * stipplePatternPixelSize;

      ${F(u,a`
          float uu = mix(u0, u0 - segmentLengthRender, isEndVertex);
          vStippleDistanceLimits = vec2(uu, uu + segmentLengthRender);
          vStipplePatternStretch = 1.0;

          // The v-coordinate used in case of an image pattern.
          bool isLeft = sign(lineParameters.y) < 0.0;
          vStippleV = isLeft ? 0.0 : 1.0;
        `,a`
          // Compute the coordinates at both start and end of the line segment, because we need both to clamp to in the
          // fragment shader
          vStippleDistanceLimits = computeStippleDistanceLimits(startPseudoScreen, segmentLengthPseudoScreen, segmentLengthScreen, patternLength);
        `)}

      vStippleDistance = mix(vStippleDistanceLimits.x, vStippleDistanceLimits.y, isEndVertex);

      // Adjust the coordinate to the displaced position (the pattern is shortened/overextended on the in/outside of
      // joins)
      if (segmentLengthScreenDouble >= EPS) {
        // Project the actual vertex position onto the line segment. Note that the resulting factor is within [0..1]
        // at the original vertex positions, and slightly outside of that range at the displaced positions
        vec3 stippleDisplacement = pos.xyz - segmentOrigin;
        float stippleDisplacementFactor = dot(segment.xy, stippleDisplacement.xy) / (segmentLengthScreenDouble * segmentLengthScreenDouble);

        // Apply this offset to the actual vertex coordinate (can be screen or pseudo-screen space)
        vStippleDistance += (stippleDisplacementFactor - isEndVertex) * (vStippleDistanceLimits.y - vStippleDistanceLimits.x);
      }

      // Cancel out perspective correct interpolation because we want this length the really represent the screen
      // distance
      vStippleDistanceLimits = noPerspectiveWrite(vStippleDistanceLimits, pos.w);
      vStippleDistance = noPerspectiveWrite(vStippleDistance, pos.w);

      // Disable stipple distance limits on caps
      vStippleDistanceLimits = isJoin ?
                                 vStippleDistanceLimits :
                                 isStartVertex ?
                                  vec2(-1e34, vStippleDistanceLimits.y) :
                                  vec2(vStippleDistanceLimits.x, 1e34);
    `)),r.main.add(a`
      // Convert back into NDC
      pos.xy = (pos.xy / viewport.zw) * pos.w;
      pos.z = pos.z * pos.w;

      vColor = getColor();
      vColor.a = noPerspectiveWrite(vColor.a * coverage, pos.w);

      ${f&&!s?"pos.z -= EPS * pos.w;":""}

      // transform final position to camera space for slicing
      vpos = (inverseProjectionMatrix * pos).xyz;
      gl_Position = pos;
      forwardObjectAndLayerIdColor();
    }`),e.fragment.include(bi,t),e.include(yi,t),o.include($i),o.main.add(a`discardBySlice(vpos);`),e.include(We),o.include(Ti),o.main.add(a`
    float lineWidth = noPerspectiveRead(vLineWidth);
    float lineDistance = noPerspectiveRead(vLineDistance);
    ${F(W,a`float lineDistanceNorm = noPerspectiveRead(vLineDistanceNorm);`)}
  `),f?o.main.add(a`vec4 finalColor = vec4(1.0, 0.0, 1.0, 1.0);`):(w&&o.main.add(a`float sdf = noPerspectiveRead(min(vSegmentSDF, vReverseSegmentSDF));
vec2 fragmentPosition = vec2(min(sdf, 0.0), lineDistance);
float fragmentRadius = length(fragmentPosition);
float fragmentCapSDF = (fragmentRadius - lineWidth) * 0.5;
float capCoverage = clamp(0.5 - fragmentCapSDF, 0.0, 1.0);
if (capCoverage < alphaCutoff) {
discard;
}`),D?o.main.add(a`vec2 stipplePosition = vec2(
min(getStippleSDF() * 2.0 - 1.0, 0.0),
lineDistanceNorm
);
float stippleRadius = length(stipplePosition * lineWidth);
float stippleCapSDF = (stippleRadius - lineWidth) * 0.5;
float stippleCoverage = clamp(0.5 - stippleCapSDF, 0.0, 1.0);
float stippleAlpha = step(alphaCutoff, stippleCoverage);`):o.main.add(a`float stippleAlpha = getStippleAlpha(lineWidth);`),l!==11&&o.main.add(a`discardByStippleAlpha(stippleAlpha, alphaCutoff);`),e.include(We),o.uniforms.add(new Ie("intrinsicColor",g=>g.color)).main.add(a`vec4 color = intrinsicColor * vColor;
color.a = noPerspectiveRead(color.a);`),h&&o.uniforms.add(new Ie("innerColor",g=>g.innerColor??g.color),new j("innerWidth",(g,y)=>g.innerWidth*y.camera.pixelRatio)).main.add(a`float distToInner = abs(lineDistance) - innerWidth;
float innerAA = clamp(0.5 - distToInner, 0.0, 1.0);
float innerAlpha = innerColor.a + color.a * (1.0 - innerColor.a);
color = mix(color, vec4(innerColor.rgb, innerAlpha), innerAA);`),o.main.add(a`vec4 finalColor = blendStipple(color, stippleAlpha);`),m&&(o.uniforms.add(new j("falloff",g=>g.falloff)),o.main.add(a`finalColor.a *= pow(max(0.0, 1.0 - abs(lineDistanceNorm)), falloff);`)),p||o.main.add(a`float stretchFactor = vIsInsideJoin == 1 ? noPerspectiveRead(vStretchFactor) : 1.0;
float featherWidth = 2.0;
float featherStartDistance = max(lineWidth - featherWidth / stretchFactor, 0.0);
float straightFeatherStartDistance = max(lineWidth - featherWidth, 0.0);
float value = abs(lineDistance);
float feather = (value - featherStartDistance) / (lineWidth - featherStartDistance);
vec2 joinCenterSDFs = noPerspectiveRead(vJoinCenterLineSDFs);
float joinCenterDistance = abs(vSubdivisionFactor > 0.5 ? joinCenterSDFs.x : joinCenterSDFs.y);
float straightFeather = (joinCenterDistance - straightFeatherStartDistance) / (lineWidth - straightFeatherStartDistance);
feather = vIsInsideJoin == 1 ? max(feather, straightFeather) : feather;
finalColor.a *= 1.0 - clamp(feather, 0.0, 1.0);`),$&&o.main.add("finalColor = animate(finalColor);")),o.main.add(a`outputColorHighlightOLID(applySlice(finalColor, vpos), finalColor.rgb);`),e}const ua=Object.freeze(Object.defineProperty({__proto__:null,build:Ut,ribbonlineNumRoundJoinSubdivisions:Be},Symbol.toStringTag,{value:"Module"}));let Ve=class extends wi{constructor(t,e){super(t,e,rn(Bt())),this.shader=new Di(ua,()=>ni(()=>Promise.resolve().then(()=>Ta),void 0)),this.ignoreUnused=!0,this.primitiveType=e.wireframe?Ue.LINES:Ue.TRIANGLE_STRIP}_makePipelineState(t,e){const{output:i,hasOccludees:n}=t;return fe({blending:_i(i,!1,t.emissionDimmingPass),depthTest:Li(i),depthWrite:zi(t),colorWrite:we,stencilWrite:n?et:null,stencilTest:n?e?Ke:Pi:null,polygonOffset:pe(t)})}initializePipeline(t){if(t.occluder){const{hasOccludees:e}=t;this._occluderPipelineTransparent=fe({blending:dt,polygonOffset:pe(t),depthTest:tt,depthWrite:null,colorWrite:we,stencilWrite:null,stencilTest:e?Ci:null}),this._occluderPipelineOpaque=fe({blending:dt,polygonOffset:pe(t),depthTest:e?tt:it,depthWrite:null,colorWrite:we,stencilWrite:e?Ei:null,stencilTest:e?Oi:null}),this._occluderPipelineMaskWrite=fe({blending:null,polygonOffset:pe(t),depthTest:it,depthWrite:null,colorWrite:null,stencilWrite:e?et:null,stencilTest:e?Ke:null})}return this._occludeePipeline=this._makePipelineState(t,!0),this._makePipelineState(t,!1)}getPipeline(t,e,i){if(i)return this._occludeePipeline;switch(t.occluder){case 11:return this._occluderPipelineTransparent??super.getPipeline(t,e,i);case 10:return this._occluderPipelineOpaque??super.getPipeline(t,e,i);default:t.occluder;case void 0:case null:return this._occluderPipelineMaskWrite??super.getPipeline(t,e,i)}}};Ve=L([ai("esri.views.3d.webgl-engine.shaders.RibbonLineTechnique")],Ve);const fa=16,ha=8;class C extends Fi{constructor(e){super(),this.spherical=e,this.capType=0,this.emissionSource=0,this.animation=2,this.polygonOffsetIndex=0,this.writeDepth=!1,this.draped=!1,this.stippleEnabled=!1,this.stippleOffColorEnabled=!1,this.stipplePreferContinuous=!0,this.numJoinSubdivisions=1,this.roundJoins=!1,this.applyMarkerOffset=!1,this.hasVVSize=!1,this.hasVVColor=!1,this.hasVVOpacity=!1,this.falloffEnabled=!1,this.innerColorEnabled=!1,this.hasOccludees=!1,this.occluder=!1,this.wireframe=!1,this.hasScreenSizePerspective=!1,this.worldSizedImagePattern=!1,this.textureCoordinateType=0,this.hasVVInstancing=!1,this.hasSliceTranslatedView=!0,this.overlayEnabled=!1,this.snowCover=!1,this.renderOccluded=!1}get hasAnimation(){return this.animation!==0}}L([_({count:3})],C.prototype,"capType",void 0),L([_({count:8})],C.prototype,"emissionSource",void 0),L([_({count:4})],C.prototype,"animation",void 0),L([_({count:fa})],C.prototype,"polygonOffsetIndex",void 0),L([_()],C.prototype,"writeDepth",void 0),L([_()],C.prototype,"draped",void 0),L([_()],C.prototype,"stippleEnabled",void 0),L([_()],C.prototype,"stippleOffColorEnabled",void 0),L([_()],C.prototype,"stipplePreferContinuous",void 0),L([_({count:ha})],C.prototype,"numJoinSubdivisions",void 0),L([_()],C.prototype,"roundJoins",void 0),L([_()],C.prototype,"applyMarkerOffset",void 0),L([_()],C.prototype,"hasVVSize",void 0),L([_()],C.prototype,"hasVVColor",void 0),L([_()],C.prototype,"hasVVOpacity",void 0),L([_()],C.prototype,"falloffEnabled",void 0),L([_()],C.prototype,"innerColorEnabled",void 0),L([_()],C.prototype,"hasOccludees",void 0),L([_()],C.prototype,"occluder",void 0),L([_()],C.prototype,"wireframe",void 0),L([_()],C.prototype,"hasScreenSizePerspective",void 0),L([_()],C.prototype,"worldSizedImagePattern",void 0);class ma extends Ai{constructor(e,i){super(e,va),this.produces=new Map([[2,n=>en(n)||Te(n)&&this.parameters.renderOccluded===8],[3,n=>tn(n)],[10,n=>ct(n)&&this.parameters.renderOccluded===8],[11,n=>ct(n)&&this.parameters.renderOccluded===8],[4,n=>Te(n)&&this.parameters.writeDepth&&this.parameters.renderOccluded!==8],[8,n=>Te(n)&&!this.parameters.writeDepth&&this.parameters.renderOccluded!==8],[18,n=>nn(n)]]),this._configuration=new C(i)}updateConfiguration(e){super.updateConfiguration(e);const i=e.slot===18,n=this.parameters.stipplePattern!=null&&this.parameters.stippleTexture!=null&&e.output!==10,r=n&&i&&this.parameters.isImagePattern();this._configuration.draped=i,this._configuration.polygonOffset=this.parameters.polygonOffset,this._configuration.stippleEnabled=n,this._configuration.stippleOffColorEnabled=n&&this.parameters.stippleOffColor!=null,this._configuration.stipplePreferContinuous=n&&this.parameters.stipplePreferContinuous,this._configuration.numJoinSubdivisions=Ht(this.parameters.join,n),this._configuration.hasSlicePlane=this.parameters.hasSlicePlane,this._configuration.roundJoins=this.parameters.join==="round",this._configuration.capType=this.parameters.cap,this._configuration.applyMarkerOffset=this.parameters.markerParameters!=null&&xa(this.parameters.markerParameters),this._configuration.polygonOffsetIndex=this.parameters.polygonOffsetIndex,this._configuration.writeDepth=this.parameters.writeDepth,this._configuration.hasVVSize=this.parameters.hasVVSize,this._configuration.hasVVColor=this.parameters.hasVVColor,this._configuration.hasVVOpacity=this.parameters.hasVVOpacity,this._configuration.innerColorEnabled=this.parameters.innerWidth>0&&this.parameters.innerColor!=null,this._configuration.falloffEnabled=this.parameters.falloff>0,this._configuration.hasOccludees=e.hasOccludees,this._configuration.occluder=this.parameters.renderOccluded===8,this._configuration.wireframe=this.parameters.wireframe,this._configuration.animation=this.parameters.animation,this._configuration.emissionSource=this.emissions?1:0,this._configuration.hasScreenSizePerspective=!!this.parameters.screenSizePerspective&&!r,this._configuration.worldSizedImagePattern=r}get visible(){return this.parameters.color[3]>=nt||this.parameters.stipplePattern!=null&&(this.parameters.stippleOffColor?.[3]??0)>nt}get emissions(){return this.parameters.emissiveStrength>0?this.parameters.renderOccluded!==8?2:1:0}setParameters(e,i){e.animation=this.parameters.animation,super.setParameters(e,i)}intersectRayDraped({attributes:e,screenToWorldRatio:i},n,r,o,c){if(!n.options.selectionMode)return;const s=this._getLineSize(e,!0),l=r[0],d=r[1],p=St(s,i);let m=Number.MAX_VALUE,v=0;for(const f of xt(G,ae,this.parameters,e)){const h=l-G[0],$=d-G[1],S=ae[0]-G[0],u=ae[1]-G[1],x=Ze((S*h+u*$)/(S*S+u*u),0,1),O=S*x-h,b=u*x-$,T=O*O+b*b;T<m&&(m=T,v=f)}m<p*p&&o(c.distance,c.renderDistance,c.normal,v)}intersectRay(e,i,n,r,o,c){const{options:s,camera:l,rayBegin:d,rayEnd:p}=n;if(!s.selectionMode||!e.visible||!l)return;if(!ot(i))return void He.getLogger("esri.views.3d.webgl-engine.materials.RibbonLineMaterial").error("intersection assumes a translation-only matrix");const m=e.attributes,v=m.get("position").data,f=this._getLineSize(m),h=ze;Ct(h,n.point);const $=f*l.pixelRatio,S=Ne*l.pixelRatio,u=$/2+S;Z(se[0],h[0]-u,h[1]+u,0),Z(se[1],h[0]+u,h[1]+u,0),Z(se[2],h[0]+u,h[1]-u,0),Z(se[3],h[0]-u,h[1]-u,0);for(let b=0;b<4;b++)if(!l.unprojectFromRenderScreen(se[b],U[b]))return;ue(l.eye,U[0],U[1],_e),ue(l.eye,U[1],U[2],Ce),ue(l.eye,U[2],U[3],Oe),ue(l.eye,U[3],U[0],Ee);let x=Number.MAX_VALUE,O=0;for(const b of this._forEachTransformedLineSegment(V,N,v,i,Se(this.parameters,m))){if(k(_e,V)<0&&k(_e,N)<0||k(Ce,V)<0&&k(Ce,N)<0||k(Oe,V)<0&&k(Oe,N)<0||k(Ee,V)<0&&k(Ee,N)<0)continue;const T=l.projectToRenderScreen(V,re),w=l.projectToRenderScreen(N,oe);if(T==null||w==null)continue;if(T[2]<0&&w[2]>0){Q(B,V,N);const z=l.frustum,g=-k(z[4],V)/ye(B,lt(z[4]));if($e(B,B,g),Ge(V,V,B),!l.projectToRenderScreen(V,T))continue}else if(T[2]>0&&w[2]<0){Q(B,N,V);const z=l.frustum,g=-k(z[4],N)/ye(B,lt(z[4]));if($e(B,B,g),Ge(N,N,B),!l.projectToRenderScreen(N,w))continue}else if(T[2]<0&&w[2]<0)continue;T[2]=0,w[2]=0;const D=ie(T,w,Le),W=Gi(D,h);if(!(W>=x)){if(this.parameters.screenSizePerspective){const z=this.computeScreenSizePerspectiveWidth(D,V,N,h,l,f,S);if(W>=z*z)continue}x=W,Re(wt,V),Re(Dt,N),O=b}}if(x<u*u){let b=Number.MAX_VALUE;if(qi(ie(wt,Dt,Le),ie(d,p,Pt),J)){Q(J,J,d);const T=qe(J);$e(J,J,1/T),b=T/ce(d,p)}c(b,b,J,O)}}intersectScreenPolygon(e,i,n,r){const{options:o,camera:c,screenPolygonPrimitiveProcessor:s}=n;if(!o.selectionMode||!e.visible)return null;if(!ot(i))return He.getLogger("esri.views.3d.webgl-engine.materials.RibbonLineMaterial").error("intersection assumes a translation-only matrix"),null;const l=e.attributes,d=l.get("position").data,p=this._getLineSize(l),m=Ne,v=m*c.pixelRatio,f=new Ii(4);for(const h of this._forEachTransformedLineSegment(V,N,d,i,Se(this.parameters,l)))for(const[$,S]of s.processLineSegment(ba,V,N)){if(!Ri(c,$,S,re,oe))continue;const u=ie(re,oe,Le);let x=p/2+m;this.parameters.screenSizePerspective&&(Ye(ze,re,oe,.5),x=Math.max(x,this.computeScreenSizePerspectiveWidth(u,$,S,ze,c,p,v)/c.pixelRatio)),c.renderToScreen(re,$t),c.renderToScreen(oe,Tt),at(r,$t,Tt,x)&&(Yi(ie($,S,Pt),c.eye,Y),f.updateIfCloserFromValues(ce(c.eye,Y),h,Y,null))}return f}intersectScreenPolygonDraped({attributes:e,screenToWorldRatio:i},n,r,o){if(!r.options.selectionMode)return null;const c=St(this._getLineSize(e,!0),i);for(const s of xt(G,ae,this.parameters,e,n))if(at(o,G,ae,c))return new Wi(s);return null}createBufferWriter(){return new Sa(Bt(this.parameters),Jt(this.parameters),this.parameters)}createGLMaterial(e){return new ga(e)}validateParameters(e){e.join!=="miter"&&(e.miterLimit=0),e.markerParameters!=null&&(e.markerScale=e.markerParameters.width/e.width)}update(e){return!!this.parameters.hasAnimation&&(this.setParameters({timeElapsed:ri(e.time)},!1),e.dt!==0)}computeScreenSizePerspectiveWidth(e,i,n,r,o,c,s){const l=Xi(e,r);Ye(Y,i,n,l),Ot(yt,Y,o.viewMatrix);const d=qe(yt),p=this.computeCameraAbsCosAngle(Y,o,this._configuration.spherical);return bt.update(p,d,this.parameters.screenSizePerspective,this.parameters.screenSizePerspectiveMinPixelReferenceSize),bt.apply(c)*o.pixelRatio/2+s}computeCameraAbsCosAngle(e,i,n){return n?Xe(J,e):Z(J,0,0,1),Q(me,e,i.eye),Xe(me,me),Math.abs(ye(J,me))}_getLineSize(e,i=!1){let n=this.parameters.width;if(this.parameters.vvSize){const r=e.get("sizeFeatureAttribute").data[0];Number.isNaN(r)?i&&(n*=this.parameters.vvSize.fallback[0]):n*=Ze(this.parameters.vvSize.offset[0]+r*this.parameters.vvSize.factor[0],this.parameters.vvSize.minSize[0],this.parameters.vvSize.maxSize[0])}else e.has("size")&&(n*=e.get("size").data[0]);return n}*_forEachTransformedLineSegment(e,i,n,r,o){const c=o?n.length-2:n.length-5;for(let s=0;s<c;s+=3){e[0]=n[s]+r[12],e[1]=n[s+1]+r[13],e[2]=n[s+2]+r[14];const l=(s+3)%n.length;i[0]=n[l]+r[12],i[1]=n[l+1]+r[13],i[2]=n[l+2]+r[14],yield s/3}}}class ga extends ji{constructor(){super(...arguments),this._stipplePattern=null}dispose(){super.dispose(),this._stippleTextures?.release(this._stipplePattern),this._stipplePattern=null}beginSlot(e){const{stipplePattern:i}=this._material.parameters;return this._stipplePattern!==i&&(this._material.setParameters({stippleTexture:this._stippleTextures.swap(i,this._stipplePattern)}),this._stipplePattern=i),this.getTechnique(Ve,e)}}class va extends Ni{constructor(){super(...arguments),this._width=0,this.color=oi,this.join="miter",this.cap=0,this.miterLimit=5,this.writeDepth=!0,this.polygonOffset=0,this.polygonOffsetIndex=0,this.stippleTexture=null,this.stipplePreferContinuous=!0,this.markerParameters=null,this.markerScale=1,this.hasSlicePlane=!1,this.vvFastUpdate=!1,this.isClosed=!1,this.falloff=0,this.innerWidth=0,this.wireframe=!1,this.timeElapsed=le(0),this.animation=0,this.animationSpeed=1,this.trailLength=1,this.startTime=le(0),this.endTime=le(1/0),this.emissiveStrength=0}get width(){return this.isImagePattern()?this.stipplePattern.width:this._width}set width(e){this._width=e}get transparent(){return this.color[3]<1||this.hasAnimation||this.stipplePattern!=null&&(this.stippleOffColor?.[3]??0)<1}get hasAnimation(){return this.animation!==0}isImagePattern(){return Wt(this.stipplePattern)&&this.stippleTexture!=null}}class Sa{constructor(e,i,n){this.layout=e,this.textureBufferLayout=i,this._parameters=n,this.numJoinSubdivisions=Ht(this._parameters.join,this._parameters.stipplePattern!=null)}_isClosed(e){return Se(this._parameters,e)}allocate(e){return this.layout.createBuffer(e)}elementCountTextureBuffer(e){return e.get("position").data.length/3+(this._isClosed(e)?3:2)}elementCount(e){const n=e.get("position").indices.length/2+1,r=this._isClosed(e);let o=r?2:4;return o+=((r?n:n-1)-(r?0:1))*(2*this.numJoinSubdivisions+4),o+=2,this._parameters.wireframe&&(o=2+4*(o-2)),o}write(e,i,n,r,o,c,s){s!=null&&this._writeTextureBuffer(e,n,r,s),o!=null&&this._writeVertexBuffer(e,n,r,o,c)}_writeTextureBuffer(e,i,n,r){const o=i.get("position"),c=o.data.length/3,s=this._isClosed(i),{buffer:l,offset:d}=r,p=l.getField("position",Bi),m=l.getField("u0",te),v=l.getField("sizeFeatureAttribute",te),f=l.getField("size",te),h=l.getField("colorFeatureAttribute",te),$=l.getField("color",st),S=l.getField("olidColor",st),u=l.getField("timeStamps",Ji),x=l.getField("opacityFeatureAttribute",te),O=i.get("sizeFeatureAttribute")?.data,b=i.get("size")?.data,T=i.get("colorFeatureAttribute")?.data,w=i.get("color")?.data,D=i.get("opacityFeatureAttribute")?.data,W=i.get("distanceToStart")?.data,z=i.get("timeStamps");H(p!==null,"Expected valid position field in texture buffer"),H(m!==null,"Expected valid u0 field in texture buffer");let g=0;for(let y=0;y<c;++y){this._getTransformedPosition(X,o.data,y,e),y>0&&(W!=null?g=W[y]??g:g+=ce(ge,X));const P=d+y+1;if(p.setVec(P,X),m.set(P,g),Re(ge,X),v&&v.set(P,O?.length===1?O[0]:O?.[y]??0),f&&f.set(P,b?.length===1?b[0]:b?.[y]??1),h&&h.set(P,T?.length===1?T[0]:T?.[y]??0),$)if(w!=null){const E=Math.min(4*y,w.length-4);$.setValues(P,w[E],w[E+1],w[E+2],w[E+3])}else $.setValues(P,1,1,1,1);x&&x.set(P,D?.length===1?D[0]:D?.[y]??0)}if(u!=null&&z!=null){H(z.size===4);const y=z.data;for(let P=0;P<c;++P){const E=4*P,M=d+P+1;u.set(M,0,y[E]),u.set(M,1,y[E+1]),u.set(M,2,y[E+2]),u.set(M,3,y[E+3])}}if(s){const y=d,P=d+1,E=d+2,M=d+c,Je=M+1,Gt=M+2;l.copyItem(M,y),l.copyItem(P,Je),l.copyItem(E,Gt),p.getVec(M,ge),p.getVec(P,X);const qt=g+ce(ge,X);m.set(Je,qt)}else{const y=d,P=d+1,E=d+c,M=E+1;l.copyItem(P,y),l.copyItem(E,M)}At()&&n!=null&&S!=null&&Mi(n,S,c+(s?3:2),d)}_writeVertexBuffer(e,i,n,r,o){const{buffer:c,offset:s}=r,{layout:l}=this,d=i.get("position"),p=d.indices,m=d.data.length/3,v=this._isClosed(i),f=m+(v?3:2);p&&p.length!==2*(m-1)&&console.warn("RibbonLineMaterial does not support indices");const h=c.getField("textureElementIndex",Ui);H(h!=null,"Missing texture buffer index field"),H(o!=null,"Using a texture layout, but the texture range for this instance was not provided");const $=o.from;H(o.numElements===f,"Expected number of elements in TextureBuffer to equal number of points");const S=new Float32Array(c.buffer),u=Hi(c.buffer),x=new Uint32Array(c.buffer),O=l.stride/4;let b=s*O;const T=b,w=S.BYTES_PER_ELEMENT/u.BYTES_PER_ELEMENT,D=(g,y,P)=>{const E=P+1;x[b]=$+E,b++;let M=b*w;u[M++]=g,u[M++]=y,b=Math.ceil(M/w)};b+=O;let W=0,z=0;v?(W=0,z=m):(D(1,-4,0),D(1,4,0),W=1,z=m-1);for(let g=W;g<z;g++){D(0,-1,g),D(0,1,g);const y=this.numJoinSubdivisions;for(let P=0;P<y;++P){const E=(P+1)/(y+1);D(E,-1,g),D(E,1,g)}D(1,-2,g),D(1,2,g)}v?(D(0,-1,z),D(0,1,z)):(D(0,-5,z),D(0,5,z)),Pe(S,T+O,S,T,O),b=Pe(S,b-O,S,b,O),this._parameters.wireframe&&this._addWireframeVertices(c,T,b,O)}_getTransformedPosition(e,i,n,r){const o=3*n;Z(e,i[o+0],i[o+1],i[o+2]),r&&Ot(e,e,r)}_addWireframeVertices(e,i,n,r){const o=new Uint8Array(e.buffer,n*Float32Array.BYTES_PER_ELEMENT),c=new Uint8Array(e.buffer,i*Float32Array.BYTES_PER_ELEMENT,(n-i)*Float32Array.BYTES_PER_ELEMENT),s=r*Float32Array.BYTES_PER_ELEMENT;let l=0;const d=p=>l=Pe(c,p,o,l,s);for(let p=0;p<=c.length-4*s;p+=2*s)d(p),d(p+2*s),d(p+1*s),d(p+2*s),d(p+1*s),d(p+3*s)}}function Pe(t,e,i,n,r){for(let o=0;o<r;o++)i[n++]=t[e++];return n}function Se(t,e){return t.isClosed?e.get("position").indices.length>2:!1}function St(t,e){return(t/2+Ne)*e}function*xt(t,e,i,n,r){const o=n.get("position").data,c=Se(i,n)?o.length-2:o.length-5,s=r?.[0]??0,l=r?.[1]??0;Ae(t,o[0]+s,o[1]+l);for(let d=3;d<c+3;d+=3){const p=d%o.length;Ae(e,o[p]+s,o[p+1]+l),yield d/3,Ct(t,e)}}function xa(t){return t.anchor===1&&t.hideOnShortSegments&&t.placement==="begin-end"&&t.worldSpace}function Ht(t,e){const i=e?1:0;switch(t){case"miter":case"bevel":return i;case"round":return Be+i}}const bt=new Vi,V=A(),N=A(),G=Me(),ae=Me(),Y=A(),yt=A(),me=A(),B=A(),J=A(),ze=A(),re=K(),oe=K(),$t=Et(),Tt=Et(),wt=A(),Dt=A(),Le=Rt(),Pt=Rt(),ge=A(),X=A(),se=[K(),K(),K(),K()],U=[A(),A(),A(),A()],_e=be(),Ce=be(),Oe=be(),Ee=be(),Ne=4,ba=[A(),A()];class Xa{constructor(e){this._originSR=e,this._rootOriginId="root/"+_t(),this._origins=new Map,this._objects=new Map,this._gridSize=5e5,this._originSR?.isGeographic&&(this._gridSize/=si(this._originSR)),this._baselineDistance=.5*this._gridSize;const i=this._baselineDistance*ya;this._baselineObjectSize=i/$a}getOrigin(e){const i=this._origins.get(this._rootOriginId);if(i==null){const p=ut(e[0],e[1],e[2],this._rootOriginId);return this._origins.set(this._rootOriginId,p),p}const n=this._gridSize,r=Math.round(e[0]/n),o=Math.round(e[1]/n),c=Math.round(e[2]/n),s=`${r}/${o}/${c}`;let l=this._origins.get(s);const d=.5*n;if(Q(R,e,i.vec3),R[0]=Math.abs(R[0]),R[1]=Math.abs(R[1]),R[2]=Math.abs(R[2]),R[0]<d&&R[1]<d&&R[2]<d){if(l){const p=Math.max(...R);if(Q(R,e,l.vec3),R[0]=Math.abs(R[0]),R[1]=Math.abs(R[1]),R[2]=Math.abs(R[2]),Math.max(...R)<p)return l}return i}return l||(l=ut(r*n,o*n,c*n,s),this._origins.set(s,l)),l}needsOriginUpdate(e,i,n){const r=ce(e.vec3,i),o=Math.max(1,n/this._baselineObjectSize);return r>this._baselineDistance*o}_drawOriginBox(e,i=li(1,1,0,1)){const n=window.view,r=n.stage,o=i.toString();if(!this._objects.has(o)){this._material=new ma({width:2,color:i},!1);const f=new ui(r,{pickable:!1}),h=new fi({castShadow:!1});f.add(h),this._objects.set(o,h)}const c=this._objects.get(o),s=[0,1,5,4,0,2,1,7,6,2,0,1,3,7,5,4,6,2,0],l=s.length,d=new Array(3*l),p=new Array,m=.5*this._gridSize;for(let f=0;f<l;f++)d[3*f]=e[0]+(1&s[f]?m:-m),d[3*f+1]=e[1]+(2&s[f]?m:-m),d[3*f+2]=e[2]+(4&s[f]?m:-m),f>0&&p.push(f-1,f);Fe(d,this._originSR,0,d,n.renderSpatialReference,0,l);const v=new ki(this._material,[["position",new hi(d,p,3,!0)]],null,2);c.addGeometry(v)}get test(){}}const R=A(),ya=2**-23,$a=.05,Ta=Object.freeze(Object.defineProperty({__proto__:null,build:Ut,ribbonlineNumRoundJoinSubdivisions:Be},Symbol.toStringTag,{value:"Module"}));export{bn as A,ma as a,Ma as b,Na as c,ja as d,Ha as e,Xa as f,Ba as g,_n as h,fa as i,Mn as j,Cn as k,Va as l,ka as m,Nt as n,We as o,hn as p,Wn as q,qa as r,jn as s,ut as t,It as u,Ga as v,la as w,ra as x};
