const __vite__mapDeps=(i,m=__vite__mapDeps,d=(m.f||(m.f=["assets/HUDMaterial.glsl-B5d9od2L.js","assets/index-lasFETI3.js","assets/index-819mKcjN.css","assets/getEmissions.glsl-DLg9S9NB.js","assets/NoParameters-DB-WZ6gy.js","assets/ShaderBuilder-B0f4Qr9Q.js","assets/OutputColorHighlightOLID.glsl-CT0JXUQY.js","assets/Indices-DQKKTg3R.js","assets/BufferView-XM-TXjbv.js","assets/sphere-D34OgoEb.js","assets/ray-DChV8pBw.js","assets/vectorStacks-D__d2ZPS.js","assets/quatf64-aQ5IuZRd.js","assets/frustum-B54TqLWG.js","assets/plane-geaygzL4.js","assets/lineSegment-DNL9grwG.js","assets/renderState-BhR1EdRH.js","assets/RibbonLine.glsl-C49thESl.js","assets/computeTranslationToOriginAndRotation-DUiWNZd9.js","assets/localRotationUtils-BmLU0eq6.js","assets/WebGLLayer-Z_IbLXDb.js","assets/mathUtils-CaTLrQDP.js","assets/Octree-C0QqT759.js","assets/Attribute-DGhdp5lO.js","assets/InterleavedLayout-CD2RdENJ.js","assets/SceneLighting-WYYKqd35.js","assets/projectVectorToVector-CQuqTceI.js","assets/projectPointToVector-BPP7T0xn.js","assets/dehydratedPoint-Z5ONvFg_.js","assets/orientedBoundingBox-BGSyqIAg.js","assets/quat-DR1FfoVB.js","assets/RenderingContext-BKyItyzg.js","assets/ProgramCache-CA3aFXvl.js","assets/VertexArrayObject-BBvFZ8MM.js","assets/VertexBuffer-XU3yhR4e.js","assets/TextureBackedBufferLayout-5Tf-J99Q.js"])))=>i.map(i=>d[i]);
import{ac as m,p as gt,bp as xt,w as St,eP as Ot,rW as Ct,kv as bt,af as Oe,_ as ke,ae as ne,dd as zt,pm as Ne,rX as ge,ha as E,kp as T,k7 as yt,cK as I,eX as ie,eU as Z,he as be,gi as Y,g6 as ze,iI as ye,eW as q,cy as we,eS as wt,fB as Pe,jc as le,dQ as Pt,gm as $t,bI as We,o as U,pb as $e,nm as At,ro as Ye,ix as Ce,ej as X,bU as Xe,rY as Vt,cI as _t,cv as Et,a_ as Ft,rZ as J,cm as Dt}from"./index-lasFETI3.js";import{u as Rt}from"./hydratedFeatures-Tmc_0VEc.js";import{i as Ut,Q as Tt}from"./BufferView-XM-TXjbv.js";import{f as It,d as jt,J as Ae,K as Mt,L as Qe,n as Ze,t as Je,S as Bt,u as Ht,g as Lt,k as qt,a as ce,M as Gt,N as Ve,O as kt,l as Nt,p as Wt,w as Yt,P as Xt,s as Qt,Q as Zt,R as Jt,I as Kt,T as ea,B as fe,U as ta,V as aa,j as ia,x as _e,W as ra,X as sa,Y as oa,Z as Ee,_ as Fe,$ as na,a0 as de,a1 as la,a2 as ca}from"./OutputColorHighlightOLID.glsl-CT0JXUQY.js";import{c as fa,a as O,r as Q,t as v,e as re,n as _,p as xe,d as da,f as K,h as ua}from"./getEmissions.glsl-DLg9S9NB.js";import{d as pa,r as ha,a as va,l as ma,b as ga}from"./BooleanBindUniform-VWNMtzUi.js";import{s as Ke,e as et,i as tt,o as xa,a as at,h as Sa,b as ee,c as ue,t as Oa}from"./SceneLighting-WYYKqd35.js";import{d as Ca}from"./VertexColor.glsl-D-XCHCPS.js";import{s as ba}from"./RibbonLine.glsl-C49thESl.js";import{c as za}from"./NoParameters-DB-WZ6gy.js";import{s as it}from"./ShaderBuilder-B0f4Qr9Q.js";import{O as rt,r as ya,g as st,u as wa}from"./renderState-BhR1EdRH.js";import{Q as ot,t as De}from"./InterleavedLayout-CD2RdENJ.js";let nt=class extends fa{constructor(){super(...arguments),this.hasEmissive=!1}};m([O()],nt.prototype,"hasEmissive",void 0);const Pa=()=>St.getLogger("esri.views.3d.layers.graphics.featureExpressionInfoUtils");function $a(i){return{cachedResult:i.cachedResult,arcade:i.arcade?{func:i.arcade.func,context:i.arcade.modules.arcadeUtils.createExecContext(null,{sr:i.arcade.context.spatialReference}),modules:i.arcade.modules}:null}}async function di(i,e,t,a){const r=i?.expression;if(typeof r!="string")return null;const s=Ea(r);if(s!=null)return{cachedResult:s};const l=await gt();xt(t);const o=l.arcadeUtils,f=o.createSyntaxTree(r);if(!f)return null;if(o.dependsOnView(f))return a?.error("Expressions containing '$view' are not supported on ElevationInfo"),{cachedResult:0};const d=o.createFunction(f);return d?{arcade:{modules:l,func:d,context:o.createExecContext(null,{sr:e})}}:null}function Aa(i,e,t){return i.arcadeUtils.createFeature(e.attributes,e.geometry,t)}function Va(i,e){if(i!=null&&!lt(i)){if(!e||!i.arcade)return void Pa().errorOncePerTick("Arcade support required but not provided");const t=e;t._geometry&&(t._geometry=Rt(t._geometry)),i.arcade.modules.arcadeUtils.updateExecContext(i.arcade.context,e)}}function _a(i){if(i!=null){if(lt(i))return i.cachedResult;const e=i.arcade;let t=e?.modules.arcadeUtils.executeFunction(e.func,e.context);return typeof t!="number"&&(i.cachedResult=0,t=0),t}return 0}function ui(i,e=!1){let t=i?.featureExpressionInfo;const a=t?.expression;return e||a==="0"||(t=null),t??null}const pi={cachedResult:0};function lt(i){return i.cachedResult!=null}function Ea(i){return i==="0"?0:null}class ct{constructor(){this._meterUnitOffset=0,this._renderUnitOffset=0,this._unit="meters",this._metersPerElevationInfoUnit=1,this._featureExpressionInfoContext=null,this.mode=null,this.centerInElevationSR=null}get featureExpressionInfoContext(){return this._featureExpressionInfoContext}get meterUnitOffset(){return this._meterUnitOffset}get unit(){return this._unit}set unit(e){this._unit=e,this._metersPerElevationInfoUnit=Ot(e)}get requiresSampledElevationInfo(){return this.mode!=="absolute-height"}reset(){this.mode=null,this._meterUnitOffset=0,this._renderUnitOffset=0,this._featureExpressionInfoContext=null,this.unit="meters"}set offsetMeters(e){this._meterUnitOffset=e,this._renderUnitOffset=0}set offsetElevationInfoUnits(e){this._meterUnitOffset=e*this._metersPerElevationInfoUnit,this._renderUnitOffset=0}addOffsetRenderUnits(e){this._renderUnitOffset+=e}geometryZWithOffset(e,t){const a=this.calculateOffsetRenderUnits(t);return this.featureExpressionInfoContext!=null?a:e+a}calculateOffsetRenderUnits(e){let t=this._meterUnitOffset;const a=this.featureExpressionInfoContext;return a!=null&&(t+=_a(a)*this._metersPerElevationInfoUnit),t/e.unitInMeters+this._renderUnitOffset}setFromElevationInfo(e){this.mode=e.mode,this.unit=Ct(e.unit)?e.unit:"meters",this.offsetElevationInfoUnits=e.offset??0}setFeatureExpressionInfoContext(e){this._featureExpressionInfoContext=e}updateFeatureExpressionInfoContextForGraphic(e,t,a){e.arcade?(this._featureExpressionInfoContext=$a(e),this.updateFeatureExpressionFeature(t,a)):this._featureExpressionInfoContext=e}updateFeatureExpressionFeature(e,t){const a=this.featureExpressionInfoContext;a?.arcade&&(a.cachedResult=void 0,Va(this._featureExpressionInfoContext,e.geometry?Aa(a.arcade.modules,e,t):null))}static fromElevationInfo(e){const t=new ct;return e!=null&&t.setFromElevationInfo(e),t}}const ft=.5;function Fa(i,e){const t=i.vertex;i.include(Ke),i.attributes.add("position","vec3"),i.vertex.inputs.add("position",()=>"position"),i.attributes.add("normal","vec3"),e.hasVertexCenterOffset?i.attributes.add("centerOffset","vec3"):t.constants.add("centerOffset","vec3",[0,0,0]),i.attributes.add("groundDistance","float"),It(t,e),jt(t,e),t.uniforms.add(new et("viewport",a=>a.camera.fullViewport),new Q("polygonOffset",a=>a.shaderPolygonOffset),new Ae("aboveGround",a=>a.camera.aboveGround?1:-1)),e.hasVerticalOffset&&pa(t),t.code.add(v`struct ProjectHUDAux {
vec3 posModel;
vec3 posView;
vec3 vnormal;
float distanceToCamera;
float absCosAngle;
};`),t.code.add(v`float applyHUDViewDependentPolygonOffset(float pointGroundDistance, float absCosAngle, inout vec3 posView) {
float pointGroundSign = sign(pointGroundDistance);
if (pointGroundSign == 0.0) {
pointGroundSign = aboveGround;
}
float groundRelative = aboveGround * pointGroundSign;
if (polygonOffset > .0) {
float cosAlpha = clamp(absCosAngle, 0.01, 1.0);
float tanAlpha = sqrt(1.0 - cosAlpha * cosAlpha) / cosAlpha;
float factor = (1.0 - tanAlpha / viewport[2]);
if (groundRelative > 0.0) {
posView *= factor;
}
else {
posView /= factor;
}
}
return groundRelative;
}`),e.draped&&!e.hasVerticalOffset||Mt(t),e.draped||(t.uniforms.add(new Ae("perDistancePixelRatio",a=>Math.tan(a.camera.fovY/2)/(a.camera.fullViewport[2]/2))),t.code.add(v`
      void applyHUDVerticalGroundOffset(vec3 normalModel, inout vec3 posModel, inout vec3 posView) {
        float distanceToCamera = length(posView);

        // Compute offset in world units for a half pixel shift
        float pixelOffset = distanceToCamera * perDistancePixelRatio * ${v.float(ft)};

        // Apply offset along normal in the direction away from the ground surface
        vec3 modelOffset = normalModel * aboveGround * pixelOffset;

        // Apply the same offset also on the view space position
        vec3 viewOffset = (viewNormal * vec4(modelOffset, 1.0)).xyz;

        posModel += modelOffset;
        posView += viewOffset;
      }
    `)),e.screenCenterOffsetUnitsEnabled&&Qe(t),e.hasScreenSizePerspective&&tt(t),t.code.add(v`
    vec4 projectPositionHUD(out ProjectHUDAux aux) {
      float pointGroundDistance = groundDistance;
      aux.posModel = position;
      aux.posView = (view * vec4(aux.posModel, 1.0)).xyz;
      aux.vnormal = normal;
      ${e.draped?"":"applyHUDVerticalGroundOffset(aux.vnormal, aux.posModel, aux.posView);"}

      // Screen sized offset in world space, used for example for line callouts
      // Note: keep this implementation in sync with the CPU implementation, see
      //   - MaterialUtil.verticalOffsetAtDistance
      //   - HUDMaterial.applyVerticalOffsetTransformation

      aux.distanceToCamera = length(aux.posView);

      vec3 viewDirObjSpace = normalize(cameraPosition - aux.posModel);
      float cosAngle = dot(aux.vnormal, viewDirObjSpace);

      aux.absCosAngle = abs(cosAngle);

      ${e.hasScreenSizePerspective&&(e.hasVerticalOffset||e.screenCenterOffsetUnitsEnabled)?"vec3 perspectiveFactor = screenSizePerspectiveScaleFactor(aux.absCosAngle, aux.distanceToCamera, screenSizePerspectiveAlignment);":""}

      ${e.hasVerticalOffset?e.hasScreenSizePerspective?"float verticalOffsetScreenHeight = applyScreenSizePerspectiveScaleFactorFloat(verticalOffset.x, perspectiveFactor);":"float verticalOffsetScreenHeight = verticalOffset.x;":""}

      ${e.hasVerticalOffset?v`
            float worldOffset = clamp(verticalOffsetScreenHeight * verticalOffset.y * aux.distanceToCamera, verticalOffset.z, verticalOffset.w);
            vec3 modelOffset = aux.vnormal * worldOffset;
            aux.posModel += modelOffset;
            vec3 viewOffset = (viewNormal * vec4(modelOffset, 1.0)).xyz;
            aux.posView += viewOffset;
            // Since we elevate the object, we need to take that into account
            // in the distance to ground
            pointGroundDistance += worldOffset;`:""}

      float groundRelative = applyHUDViewDependentPolygonOffset(pointGroundDistance, aux.absCosAngle, aux.posView);

      ${e.screenCenterOffsetUnitsEnabled?"":v`
            // Apply x/y in view space, but z in screen space (i.e. along posView direction)
            aux.posView += vec3(centerOffset.x, centerOffset.y, 0.0);

            // Same material all have same z != 0.0 condition so should not lead to
            // branch fragmentation and will save a normalization if it's not needed
            if (centerOffset.z != 0.0) {
              aux.posView -= normalize(aux.posView) * centerOffset.z;
            }
          `}

      vec4 posProj = proj * vec4(aux.posView, 1.0);

      ${e.screenCenterOffsetUnitsEnabled?e.hasScreenSizePerspective?"float centerOffsetY = applyScreenSizePerspectiveScaleFactorFloat(centerOffset.y, perspectiveFactor);":"float centerOffsetY = centerOffset.y;":""}

      ${e.screenCenterOffsetUnitsEnabled?"posProj.xy += vec2(centerOffset.x, centerOffsetY) * pixelRatio * 2.0 / viewport.zw * posProj.w;":""}

      // constant part of polygon offset emulation
      posProj.z -= groundRelative * polygonOffset * posProj.w;
      return posProj;
    }
  `)}function Da(i){i.uniforms.add(new ha("alignPixelEnabled",e=>e.alignPixelEnabled)),i.code.add(v`vec4 alignToPixelCenter(vec4 clipCoord, vec2 widthHeight) {
if (!alignPixelEnabled)
return clipCoord;
vec2 xy = vec2(0.500123) + 0.5 * clipCoord.xy / clipCoord.w;
vec2 pixelSz = vec2(1.0) / widthHeight;
vec2 ij = (floor(xy * widthHeight) + vec2(0.5)) * pixelSz;
vec2 result = (ij * 2.0 - vec2(1.0)) * clipCoord.w;
return vec4(result, clipCoord.zw);
}`),i.code.add(v`vec4 alignToPixelOrigin(vec4 clipCoord, vec2 widthHeight) {
if (!alignPixelEnabled)
return clipCoord;
vec2 xy = vec2(0.5) + 0.5 * clipCoord.xy / clipCoord.w;
vec2 pixelSz = vec2(1.0) / widthHeight;
vec2 ij = floor((xy + 0.5 * pixelSz) * widthHeight) * pixelSz;
vec2 result = (ij * 2.0 - vec2(1.0)) * clipCoord.w;
return vec4(result, clipCoord.zw);
}`)}class dt extends za{constructor(){super(...arguments),this.effect=0,this.fadeFactor=bt(1)}}function Ra(i){const e=new it;return e.include(xa),e.outputs.add("fragColor","vec4",0),e.fragment.uniforms.add(new re("colorTexture",t=>t.color),new re("focusArea",t=>t.focusArea),new at("focusAreaEffectMode",t=>t.effect),new Q("fadeFactor",t=>t.fadeFactor.value)).constants.add("EffectBright","int",0).main.add(`
      float mask = texture(focusArea, uv).r;
      
      if (focusAreaEffectMode == EffectBright) {
        vec4 color = texture(colorTexture, uv);
        float luminance = color.r * 0.25 + color.g * 0.5 + color.b * 0.25;
        fragColor = mask > 0.0 ? color : mix(color, vec4(0.55 * luminance + 0.45), fadeFactor);
      } else {
        if(mask > 0.0) discard;
        fragColor = vec4(vec3(0.0), fadeFactor * 0.67);
      }
  `),i.hasEmissive&&(e.outputs.add("fragEmission","vec4",1),e.fragment.main.add(`
      if (focusAreaEffectMode == EffectBright)
        fragEmission = vec4(vec3(0.0), fadeFactor * 0.67);
      else
        fragEmission = vec4(vec3(0.0), fadeFactor * 0.9);
    `)),e}const Ua=Object.freeze(Object.defineProperty({__proto__:null,FocusAreaColorPassParameters:dt,build:Ra},Symbol.toStringTag,{value:"Module"}));let se=class extends Ze{constructor(){super(...arguments),this.shader=new Je(Ua,()=>ke(()=>import("./HUDMaterial.glsl-B5d9od2L.js").then(i=>i.F),__vite__mapDeps([0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35]))),this.ignoreUnused=!0}initializePipeline(){return rt({colorWrite:st,blending:ya})}};se=m([Oe("esri.views.3d.webgl-engine.effects.focusArea.FocusAreaColorTechnique")],se);let G=class extends Sa{constructor(i){super({...i,view:i.focusAreasView.view}),this.consumes={required:[ee.FOCUSAREA_COLOR,ee.FOCUSAREA]},this.produces=ee.FOCUSAREA_COLOR,this._fadeDirection=0,this._configuration=new nt,this._passParameters=new dt}fadeOut(i){this.removeAllHandles(),this._startTime=null,this._fadeDirection=1,this.addHandles(zt(()=>this._passParameters.fadeFactor.value,e=>{e===0&&(this.removeAllHandles(),i())})),this.requestRender(2)}precompile(){this._configuration.hasEmissive=this.bindParameters.emissions!==0,this.techniques.precompile(se,this._configuration)}render(i){const e=this.bindParameters;this._startTime??=this.view.stage?.renderer.renderContext.time;const t=this.view.qualitySettings.fadeDuration,a=t>0?Math.min(t,this.view.stage?.renderer.renderContext.time-this._startTime)/t:1,r=this.renderingContext,s=this.techniques.get(se,this._configuration),l=this.input,o=i.find(({name:S})=>S===ee.FOCUSAREA),f=this.focusAreasView.style==="bright";this._passParameters.color=f?l.getTexture():r.emptyTexture,this._passParameters.focusArea=o.getTexture(),this._passParameters.effect=ut[this.focusAreasView.style],this._passParameters.fadeFactor.value=this._fadeDirection===0?a:1-a;const d=e.camera,n=d.fullViewport[2],u=d.fullViewport[3],h=f?this.fboCache.acquire(n,u,this.produces).moveAttachments(l):l;return r.bindFramebuffer(h.fbo),f&&r.clearBuffer(0,Ne),r.bindTechnique(s,e,this._passParameters),r.screen.draw(),a<1&&this.requestRender(2),h}};m([ne()],G.prototype,"consumes",void 0),m([ne()],G.prototype,"produces",void 0),m([ne({constructOnly:!0})],G.prototype,"focusAreasView",void 0),G=m([Oe("esri.views.3d.webgl-engine.effects.focusArea.FocusAreaVisualization")],G);const ut={bright:0,dark:1},Ta=i=>i?ut[i]:0;function Ia(i){const e=new it;e.include(Fa,i),e.vertex.include(Bt,i);const{output:t,hasOcclusionTexture:a,signedDistanceFieldEnabled:r,pixelSnappingEnabled:s,hasEmission:l,hasScreenSizePerspective:o,debugDrawLabelBorder:f,hasVVSize:d,hasVVColor:n,hasRotation:u,occludedFragmentFade:h,sampleSignedDistanceFieldTexelCenter:S,hasVertexColor:g,hasVertexSize:A,hasVertexRotation:F,hasVertexUVi:w}=i;e.include(Ke),e.include(Ht,i),e.include(Ca,i),e.include(Lt,i);const{vertex:x,fragment:p}=e;p.include(qt),p.code.add(v`
    vec4 applyFocusAreaStyle(vec4 color, int style) {
      const float factor = 0.46;
      const float factorBright = 0.32;

      if (style == ${v.int(0)}) {
        float luma = (color.r + color.g + color.b) / 3.0;
        float bright = luma * (1.0 - 0.6 * factorBright) + 0.6 * factorBright * color.a;
        float brightScaled = bright * factorBright;
        return vec4(brightScaled, brightScaled, brightScaled, color.a * factorBright);
      }

      float darkScaled = factor * factor;
      return vec4(color.rgb * darkScaled, color.a * factor);
    }
  `),e.varyings.add("vcolor","vec4"),e.varyings.add("vtc","vec2"),e.varyings.add("vsize","vec2");const b=t===10;x.uniforms.add(new et("viewport",c=>c.camera.fullViewport),new ue("screenOffset",(c,L)=>T(ae,2*c.screenOffset[0]*L.camera.pixelRatio,2*c.screenOffset[1]*L.camera.pixelRatio)),new ue("anchorPosition",c=>oe(c)),new ce("materialColor",({color:c})=>c),new Q("materialRotation",c=>c.rotation),new ue("materialSize",c=>c.size),new re("tex",c=>c.texture)),Qe(x),r&&(x.uniforms.add(new ce("outlineColor",c=>c.outlineColor)),p.uniforms.add(new ce("outlineColor",c=>Re(c)?c.outlineColor:Ne),new Q("outlineSize",c=>Re(c)?c.outlineSize:0))),s&&x.include(Da),o&&(Oa(x),tt(x)),f&&e.varyings.add("debugBorderCoords","vec4"),e.attributes.add("uv0","vec2"),w&&e.attributes.add("uvi","vec4"),g&&e.attributes.add("color","vec4"),A&&e.attributes.add("size","vec2"),F&&e.attributes.add("rotation","float"),(d||n)&&e.attributes.add("featureAttribute","vec4"),x.main.add(v`
    ProjectHUDAux projectAux;
    vec4 posProj = projectPositionHUD(projectAux);
    forwardObjectAndLayerIdColor();

    if (rejectBySlice(projectAux.posModel)) {
      gl_Position = ${ba};
      return;
    }

    vec2 vertexSize = materialSize${_(A," * size")};
    vec2 inputSize;
    ${_(o,v`
        inputSize = screenSizePerspectiveScaleVec2(vertexSize, projectAux.absCosAngle, projectAux.distanceToCamera, screenSizePerspective);
        vec2 screenOffsetScaled = screenSizePerspectiveScaleVec2(screenOffset, projectAux.absCosAngle, projectAux.distanceToCamera, screenSizePerspectiveAlignment);`,v`
        inputSize = vertexSize;
        vec2 screenOffsetScaled = screenOffset;`)}
    ${_(d,v`inputSize *= vvScale(featureAttribute).xx;`)}

    vec2 combinedSize = inputSize * pixelRatio;
    vec4 quadOffset = vec4(0.0);
  `);const V=v`
  ${_(w,v`
    vec2 texSize = vec2(textureSize(tex, 0));
    vec2 uv = mix(uvi.xy, uvi.zw, bvec2(uv0)) / texSize;
    `,v`
    vec2 uv = mix(vec2(0.), vec2(1.), bvec2(uv0));
    `)}

    quadOffset.xy = (uv0 - anchorPosition) * 2.0 * combinedSize;

    ${_(u,v`
        float angle = radians(materialRotation${_(F," + rotation")});
        float cosAngle = cos(angle);
        float sinAngle = sin(angle);
        mat2 rotate = mat2(cosAngle, -sinAngle, sinAngle,  cosAngle);

        quadOffset.xy = rotate * quadOffset.xy;
      `)}

    quadOffset.xy = (quadOffset.xy + screenOffsetScaled) / viewport.zw * posProj.w;
  `,R=s?r?v`posProj = alignToPixelOrigin(posProj, viewport.zw) + quadOffset;`:v`posProj += quadOffset;
if (inputSize.x == vertexSize.x) {
posProj = alignToPixelOrigin(posProj, viewport.zw);
}`:v`posProj += quadOffset;`;x.include(Gt),x.main.add(v`
    ${V}
    ${n?"vcolor = interpolateVVColor(featureAttribute.y) * materialColor;":g?"vcolor = color * materialColor;":"vcolor = materialColor;"}

    ${_(t===11,v`vcolor.a = 1.0;`)}

    bool alphaDiscard = vcolor.a < alphaCutoff;
    ${_(r,"alphaDiscard = alphaDiscard && outlineColor.a < alphaCutoff;")}
    if (alphaDiscard) {
      // "early discard" if both symbol color (= fill) and outline color (if applicable) are transparent
      gl_Position = vec4(1e38, 1e38, 1e38, 1.0);
      return;
    } else {
      ${R}
      gl_Position = posProj;
    }

    vtc = uv;

    ${_(f,v`debugBorderCoords = vec4(uv0, 1.5 / combinedSize);`)}
    vsize = inputSize;
  `);const D=xe(t)&&i.hasFocusAreaStyle&&!i.draped;switch(p.uniforms.add(new re("tex",c=>c.texture)),D&&p.uniforms.add(new at("focusAreaStyle",c=>Ta(c.focusAreaStyle))),h&&!b&&(p.include(va),p.uniforms.add(new Ve("depthMap",c=>c.mainDepth),new Q("occludedOpacity",c=>c.occludedFragmentOpacity?.value??1))),a&&p.uniforms.add(new Ve("texOcclusion",c=>c.hudOcclusion?.attachment)),f?p.main.add(`
        float isBorder = float(any(lessThan(debugBorderCoords.xy, debugBorderCoords.zw)) || any(greaterThan(debugBorderCoords.xy, 1.0 - debugBorderCoords.zw)));
        // don't discard fragments on debug border
        float textureAlphaCutoff = isBorder > 0.0 ? 0.0 : alphaCutoff;
      `):p.main.add("float textureAlphaCutoff = alphaCutoff;"),p.main.add("vec2 samplePos = vtc;"),S&&p.main.add(v`float txSize = float(textureSize(tex, 0).x);
float texelSize = 1.0 / txSize;
vec2 scaleFactor = (vsize - txSize) * texelSize;
samplePos += (vec2(1.0, -1.0) * texelSize) * scaleFactor;`),r?p.main.add(v`
      vec4 fillPixelColor = vcolor;

      // Get distance in output units (i.e. pixels)

      float sdf = texture(tex, samplePos).r;
      float pixelDistance = sdf * vsize.x;

      // Create smooth transition from the icon into its outline
      float fillAlphaFactor = clamp(0.5 - pixelDistance, 0.0, 1.0);
      fillPixelColor.a *= fillAlphaFactor;

      if (outlineSize > 0.25) {
        vec4 outlinePixelColor = outlineColor;
        float clampedOutlineSize = min(outlineSize, 0.5*vsize.x);

        // Create smooth transition around outline
        float outlineAlphaFactor = clamp(0.5 - (abs(pixelDistance) - 0.5*clampedOutlineSize), 0.0, 1.0);
        outlinePixelColor.a *= outlineAlphaFactor;

        if (
          outlineAlphaFactor + fillAlphaFactor < textureAlphaCutoff ||
          fillPixelColor.a + outlinePixelColor.a < alphaCutoff
        ) {
          discard;
        }

        // perform un-premultiplied over operator (see https://en.wikipedia.org/wiki/Alpha_compositing#Description)
        float compositeAlpha = outlinePixelColor.a + fillPixelColor.a * (1.0 - outlinePixelColor.a);
        vec3 compositeColor = vec3(outlinePixelColor) * outlinePixelColor.a +
                              vec3(fillPixelColor) * fillPixelColor.a * (1.0 - outlinePixelColor.a);

        ${_(!b,v`fragColor = vec4(compositeColor, compositeAlpha);`)}
      } else {
        if (fillAlphaFactor < textureAlphaCutoff) {
          discard;
        }

        ${_(!b,v`fragColor = premultiplyAlpha(fillPixelColor);`)}
      }

      // visualize SDF:
      // fragColor = vec4(clamp(-pixelDistance/vsize.x*2.0, 0.0, 1.0), clamp(pixelDistance/vsize.x*2.0, 0.0, 1.0), 0.0, 1.0);
      `):p.main.add(v`
        vec4 texColor = texture(tex, samplePos, -0.5);
        if (texColor.a < textureAlphaCutoff) {
          discard;
        }
        ${_(!b,v`fragColor = texColor * premultiplyAlpha(vcolor);`)}
      `),h&&!b&&p.main.add(v`
        float zSample = -linearizeDepth(texelFetch(depthMap, ivec2(gl_FragCoord.xy), 0).x);
        float zFragment = -linearizeDepth(gl_FragCoord.z);
        if (zSample < ${v.float(1-Ma)} * zFragment) {
          fragColor *= occludedOpacity;
        }
      `),a&&p.main.add("fragColor *= texelFetch(texOcclusion, ivec2(gl_FragCoord.xy), 0).r;"),!b&&f&&p.main.add("fragColor = mix(fragColor, vec4(1.0, 0.0, 1.0, 1.0), isBorder * 0.5);"),t===2&&p.main.add(v`if (fragColor.a < alphaCutoff) {
discard;
}`),D&&p.main.add(v`fragColor = applyFocusAreaStyle(fragColor, focusAreaStyle);`),xe(t)&&l&&p.main.add("fragEmission = vec4(0.0);"),t){case 1:p.main.add(`
        fragColor = vec4(fragColor.rgb * floatBlendOutputScale, fragColor.a);
        fragAlpha = fragColor.a * floatBlendOutputScale;
      `);break;case 2:p.main.add("fragColor.rgb /= fragColor.a;");break;case 11:p.main.add("outputObjectAndLayerIdColor();");break;case 10:e.include(kt,i),p.main.add("outputHighlight(false);")}return e}function Re(i){return i.outlineColor[3]>0&&i.outlineSize>0}function oe(i){return i.textureIsSignedDistanceField?ja(i.anchorPosition,i.distanceFieldBoundingBox,ae):ge(ae,i.anchorPosition),ae}const ae=E();function ja(i,e,t){T(t,i[0]*(e[2]-e[0])+e[0],i[1]*(e[3]-e[1])+e[1])}const Ma=.08,Ba=Object.freeze(Object.defineProperty({__proto__:null,anchorPosition:oe,build:Ia},Symbol.toStringTag,{value:"Module"}));let Se=class extends Ze{constructor(i,e){super(i,e,De(pt).concat(De(ht(e)))),this.shader=new Je(Ba,()=>ke(()=>import("./HUDMaterial.glsl-B5d9od2L.js").then(t=>t.H),__vite__mapDeps([0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35]))),this.ignoreUnused=!0,this.primitiveType=yt.TRIANGLE_STRIP}initializePipeline(i){const{draped:e,output:t,depthTestEnabled:a}=i,r=da(t),s=a&&!e&&!r&&t!==10;return rt({blending:Yt(t,!0),depthTest:a&&!e?{func:515}:null,depthWrite:s?wa:null,colorWrite:st,polygonOffset:Wt(i)})}};Se=m([Oe("esri.views.3d.webgl-engine.shaders.HUDMaterialTechnique")],Se);const pt=ot().vec2u8("uv0",{glNormalized:!0});function ht(i){let e=ot().vec3f("position").vec3f("normal").f32("groundDistance");return i.hasVertexCenterOffset&&(e=e.vec3f("centerOffset")),i.hasVertexColor&&(e=e.vec4u8("color",{glNormalized:!0})),i.hasVertexSize&&(e=e.vec2f("size")),i.hasVertexRotation&&(e=e.f32("rotation")),(i.hasVVColor||i.hasVVSize)&&(e=e.vec4f("featureAttribute")),i.hasVertexUVi&&(e=e.vec4i16("uvi")),Nt()?e.vec4u8("olidColor"):e}class C extends Xt{constructor(e,t){super(),this.spherical=e,this.polygonOffset=0,this.enableOITOffset=!1,this.screenCenterOffsetUnitsEnabled=!1,this.signedDistanceFieldEnabled=!1,this.sampleSignedDistanceFieldTexelCenter=!1,this.hasVVSize=!1,this.hasVVColor=!1,this.hasVerticalOffset=!1,this.hasScreenSizePerspective=!1,this.hasRotation=!1,this.debugDrawLabelBorder=!1,this.depthTestEnabled=!0,this.pixelSnappingEnabled=!0,this.draped=!1,this.occludedFragmentFade=!1,this.hasOcclusionTexture=!1,this.hasFocusAreaStyle=!1,this.hasVertexColor=!0,this.hasVertexSize=!0,this.hasVertexRotation=!0,this.hasVertexUVi=!0,this.hasVertexCenterOffset=!0,this.olidColorInstanced=!1,this.textureCoordinateType=0,this.emissionSource=0,this.hasVVInstancing=!1,this.snowCover=!1,this.renderOccluded=!1,this.transparentOccluded=t}}m([O()],C.prototype,"transparentOccluded",void 0),m([O({count:5})],C.prototype,"polygonOffset",void 0),m([O()],C.prototype,"enableOITOffset",void 0),m([O()],C.prototype,"screenCenterOffsetUnitsEnabled",void 0),m([O()],C.prototype,"signedDistanceFieldEnabled",void 0),m([O()],C.prototype,"sampleSignedDistanceFieldTexelCenter",void 0),m([O()],C.prototype,"hasVVSize",void 0),m([O()],C.prototype,"hasVVColor",void 0),m([O()],C.prototype,"hasVerticalOffset",void 0),m([O()],C.prototype,"hasScreenSizePerspective",void 0),m([O()],C.prototype,"hasRotation",void 0),m([O()],C.prototype,"debugDrawLabelBorder",void 0),m([O()],C.prototype,"depthTestEnabled",void 0),m([O()],C.prototype,"pixelSnappingEnabled",void 0),m([O()],C.prototype,"draped",void 0),m([O()],C.prototype,"occludedFragmentFade",void 0),m([O()],C.prototype,"hasOcclusionTexture",void 0),m([O()],C.prototype,"hasFocusAreaStyle",void 0),m([O()],C.prototype,"hasVertexColor",void 0),m([O()],C.prototype,"hasVertexSize",void 0),m([O()],C.prototype,"hasVertexRotation",void 0),m([O()],C.prototype,"hasVertexUVi",void 0),m([O()],C.prototype,"hasVertexCenterOffset",void 0);class hi extends Qt{constructor(e,t,a=!1){super(e,Ya),this.produces=new Map([[12,r=>K(r)&&!this.parameters.drawAsLabel&&!this._configuration.transparentOccluded],[13,r=>K(r)&&!this.parameters.drawAsLabel&&this._configuration.transparentOccluded],[14,r=>K(r)&&this.parameters.drawAsLabel],[18,r=>this.parameters.draped&&K(r)]]),this._visible=!0,this._configuration=new C(t,a)}updateConfiguration(e){super.updateConfiguration(e);const{parameters:t,_configuration:a}=this,r=t.draped;a.enableOITOffset=e.enableOITOffset,a.hasSlicePlane=this.parameters.hasSlicePlane,a.hasVerticalOffset=!!this.parameters.verticalOffset,a.hasScreenSizePerspective=!!this.parameters.screenSizePerspective,a.screenCenterOffsetUnitsEnabled=this.parameters.centerOffsetUnits==="screen",a.polygonOffset=this.parameters.polygonOffset,a.draped=r,a.pixelSnappingEnabled=this.parameters.pixelSnappingEnabled,a.signedDistanceFieldEnabled=this.parameters.textureIsSignedDistanceField,a.sampleSignedDistanceFieldTexelCenter=this.parameters.sampleSignedDistanceFieldTexelCenter,a.hasRotation=this.parameters.hasRotation,a.hasVVSize=!!this.parameters.vvSize,a.hasVVColor=!!this.parameters.vvColor,a.occludedFragmentFade=!r&&!!this.parameters.occludedFragmentOpacity,a.hasFocusAreaStyle=this.parameters.focusAreaStyle!=null,a.depthTestEnabled=this.parameters.depthEnabled,a.hasVertexColor=this.parameters.hasVertexColor,a.hasVertexSize=this.parameters.hasVertexSize,a.hasVertexRotation=this.parameters.hasVertexRotation,a.hasVertexUVi=this.parameters.hasVertexUVi,a.hasVertexCenterOffset=this.parameters.hasVertexCenterOffset,xe(e.output)&&(a.debugDrawLabelBorder=!!Zt.LABELS_SHOW_BORDER),a.hasOcclusionTexture=!t.drawAsLabel&&a.transparentOccluded&&ua(e.output)}intersectRay(e,t,a,r,s,l){const{options:{selectionMode:o,hud:f,excludeLabels:d},point:n,camera:u}=a,{parameters:h}=this;if(o&&f&&(!d||!h.isLabel)&&e.visible&&n&&u){for(const{renderScreenPosition:S,screenSize:g,pixelRatioTolerance:A,halfOutlineSize:F,rotationAngle:w,anchor:x,centerView:p}of this._forEachScreenSpaceHUDInstance(Be,e,t,u))if(Te(n,S[0],S[1],g,A,F,w,h,x)){const b=a.ray;if(te[0]=n[0],te[1]=n[1],te[2]=S[2],u.unprojectFromRenderScreen(te,z)){const V=U();I(V,b.direction);const R=1/ie(V);Z(V,V,R);const D=be(b.origin,z)*R,c=U();Y(c,p,u.inverseViewMatrix),l(D,D,V,-1,c)}}}}*_forEachScreenSpaceHUDInstance(e,t,a,r){const{parameters:s}=this,l=t.attributes.get("featureAttribute"),o=l==null?null:ze(l.data,Me),{scaleX:f,scaleY:d}=qe(o,s,r.pixelRatio),n=t.attributes.get("position"),u=t.attributes.get("size"),h=t.attributes.get("normal"),S=t.attributes.get("rotation"),g=t.attributes.get("centerOffset"),A=s.size,F=ye(je,a);Ut(n.size>=3);const w=s.centerOffsetUnits==="screen";for(let x=0;x<n.data.length/n.size;x++){const p=x*n.size;if(q(z,n.data[p],n.data[p+1],n.data[p+2]),Y(z,z,a),Y(z,z,r.viewMatrix),g){const c=x*g.size;q($,g.data[c],g.data[c+1],g.data[c+2])}else q($,0,0,0);if(!w&&(z[0]+=$[0],z[1]+=$[1],$[2]!==0)){const c=$[2];we($,z),wt(z,z,Z($,$,c))}const b=x*h.size;q(j,h.data[b],h.data[b+1],h.data[b+2]),Pe(j,j,F);const{normal:V,cosAngle:R}=Ue(j,r,Le),D=Ge(s,z,R,r,pe);if(le(z,z,V,D),r.applyProjection(z,P),P[0]>-1){if(w&&($[0]||$[1])&&(P[0]+=$[0]*r.pixelRatio,$[1]!==0&&(P[1]+=pe.alignmentEvaluator.apply($[1])*r.pixelRatio),r.unapplyProjection(P,z)),P[0]+=s.screenOffset[0]*r.pixelRatio,P[1]+=s.screenOffset[1]*r.pixelRatio,P[0]=Math.floor(P[0]),P[1]=Math.floor(P[1]),y[0]=A[0],y[1]=A[1],u!=null){const L=x*u.size;y[0]*=u.data[L],y[1]*=u.data[L+1]}pe.evaluator.applyVec2(y,y);let c=0;s.textureIsSignedDistanceField&&(c=Math.min(s.outlineSize,.5*y[0])*r.pixelRatio/2),y[0]*=f,y[1]*=d,I(e.centerView,z),q(e.renderScreenPosition,P[0],P[1],P[2]),ge(e.screenSize,y),e.pixelRatioTolerance=ka*r.pixelRatio,e.halfOutlineSize=c,e.rotationAngle=s.rotation+(S!=null?S.data[x*S.size]:0),e.anchor=oe(s),yield e}}}*_forEachDrapedHUDInstance(e,t){const a=t.attributes.get("position"),r=t.attributes.get("size"),s=t.attributes.get("rotation"),l=this.parameters,o=l.size,f=t.attributes.get("featureAttribute"),d=f==null?null:ze(f.data,Me),{scaleX:n,scaleY:u}=qe(d,l,t.screenToWorldRatio);e.pixelRatioTolerance=Na*t.screenToWorldRatio,e.anchor=oe(l);for(let h=0;h<a.data.length/a.size;h++){const S=h*a.size;if(e.x=a.data[S],e.y=a.data[S+1],y[0]=o[0],y[1]=o[1],r!=null){const g=h*r.size;y[0]*=r.data[g],y[1]*=r.data[g+1]}if(e.halfOutlineSize=0,l.textureIsSignedDistanceField){const g=Math.min(l.outlineSize,.5*y[0]);e.halfOutlineSize=g*t.screenToWorldRatio/2}y[0]*=n,y[1]*=u,ge(e.screenSize,y),e.rotationAngle=l.rotation+(s!=null?s.data[h*s.size]:0),yield e}}intersectRayDraped(e,t,a,r,s){const l=this.parameters;for(const{x:o,y:f,screenSize:d,pixelRatioTolerance:n,halfOutlineSize:u,rotationAngle:h,anchor:S}of this._forEachDrapedHUDInstance(He,e))Te(a,o,f,d,n,u,h,l,S)&&r(s.distance,s.renderDistance,s.normal,-1)}intersectScreenPolygonDraped(e,t,a,r){if(!a.options.selectionMode||a.options.excludeLabels&&this.parameters.isLabel)return null;const s=this.parameters;for(const{x:l,y:o,screenSize:f,pixelRatioTolerance:d,halfOutlineSize:n,rotationAngle:u,anchor:h}of this._forEachDrapedHUDInstance(He,e))if(Ie(r,l+t[0],o+t[1],f,d,n,u,s,h,!1))return new Kt(-1);return null}intersectScreenPolygon(e,t,a,r){const{options:{selectionMode:s,hud:l,excludeLabels:o},camera:f}=a,{parameters:d}=this;if(!s||!l||o&&d.isLabel||!e.visible||t==null)return null;const n=new ea(1),u=1/f.pixelRatio;for(const{renderScreenPosition:h,screenSize:S,pixelRatioTolerance:g,halfOutlineSize:A,rotationAngle:F,anchor:w,centerView:x}of this._forEachScreenSpaceHUDInstance(Be,e,t,f)){const p=Y(z,x,f.inverseViewMatrix);if(!a.screenPolygonPrimitiveProcessor?.validatePoint(p))continue;const b=h[0]*u,V=(f.fullHeight-h[1])*u;if(me[0]=S[0]*u,me[1]=S[1]*u,Ie(r,b,V,me,g*u,A*u,F,d,w,!0)){const R=be(f.eye,p);n.updateIfCloserFromValues(R,-1,p,null)}}return n.valid?n:null}createBufferWriter(){return new Xa(this.parameters)}applyShaderOffsets(e,t,a,r,s,l,o,f){Pe(he,a,ye(je,r));const d=Ue(he,o,Le),n=Qa(ie(t),o),u=Ge(this.parameters,t,d.cosAngle,o,f);le(t,t,d.normal,u+n),le(e,e,he,u+n);const h=l+u;this._applyPolygonOffsetView(t,d,h,o,t),this._applyCenterOffsetView(t,s,t)}applyShaderOffsetsNDC(e,t,a,r,s,l){return this._applyCenterOffsetNDC(e,t,r,s),l!=null&&I(l,s),this._applyPolygonOffsetNDC(s,a,r,s),s}_applyPolygonOffsetView(e,t,a,r,s){const l=r.aboveGround?1:-1;let o=Math.sign(a);o===0&&(o=l);const f=l*o;if(this.parameters.shaderPolygonOffset<=0)return I(s,e);const d=Pt(Math.abs(t.cosAngle),.01,1),n=1-Math.sqrt(1-d*d)/d/r.viewport[2];return Z(s,e,f>0?n:1/n),s}_applyCenterOffsetView(e,t,a){const r=this.parameters.centerOffsetUnits!=="screen";return a!==e&&I(a,e),r&&(a[0]+=t[0],a[1]+=t[1],t[2]&&(we(j,a),$t(a,a,Z(j,j,t[2])))),a}_applyCenterOffsetNDC(e,t,a,r){const s=this.parameters.centerOffsetUnits!=="screen";return r!==e&&I(r,e),s||(r[0]+=t[0]/a.fullWidth*2,r[1]+=t[1]/a.fullHeight*2),r}_applyPolygonOffsetNDC(e,t,a,r){const s=this.parameters.shaderPolygonOffset;if(e!==r&&I(r,e),s){const l=a.aboveGround?1:-1,o=l*Math.sign(t);r[2]-=(o||l)*s}return r}set visible(e){this._visible=e}get visible(){const{color:e,outlineSize:t,outlineColor:a}=this.parameters,r=e[3]>=fe||t>=fe&&a[3]>=fe;return this._visible&&r}createGLMaterial(e){return new Ha(e)}calculateRelativeScreenBounds(e,t,a=We()){return La(this.parameters,e,t,a),a[2]=a[0]+e[0],a[3]=a[1]+e[1],a}}class Ha extends ga{constructor(e){super({...e,...e.material.parameters})}beginSlot(e){return this.updateTexture(this._material.parameters.textureId),this._material.setParameters(this.textureBindParameters),this.getTechnique(Se,e)}}function La(i,e,t,a){a[0]=i.anchorPosition[0]*-e[0]+i.screenOffset[0]*t,a[1]=i.anchorPosition[1]*-e[1]+i.screenOffset[1]*t}function Ue(i,e,t){return Y(t.normal,i,e.viewInverseTransposeMatrix),t.cosAngle=Et(t.normal,Wa),t}function Te(i,e,t,a,r,s,l,o,f){const d=vt(e,t,a,r,s,o,f,mt);return T(M,e,t),X(k,i,M,Xe(l)),k[0]>d[0]&&k[0]<d[2]&&k[1]>d[1]&&k[1]<d[3]}function Ie(i,e,t,a,r,s,l,o,f,d){const n=vt(e,t,a,r,s,o,f,mt,d);if(T(B,n[0],n[1]),T(N,n[2],n[1]),T(H,n[2],n[3]),T(W,n[0],n[3]),l!==0){const u=Xe(l);T(M,e,t),X(B,B,M,u),X(N,N,M,u),X(H,H,M,u),X(W,W,M,u),Ft(n),J(n,B),J(n,N),J(n,H),J(n,W)}return!!ia(i,n)&&(_e(i,B,N,H)||_e(i,B,H,W))}function vt(i,e,t,a,r,s,l,o,f=!1){let d=i-a-t[0]*l[0],n=d+t[0]+2*a;const u=f?1-l[1]:l[1];let h=e-a-t[1]*u,S=h+t[1]+2*a;const g=s.distanceFieldBoundingBox;return s.textureIsSignedDistanceField&&g!=null&&(d+=t[0]*g[0],h+=t[1]*(f?1-g[3]:g[1]),n-=t[0]*(1-g[2]),S-=t[1]*(f?g[1]:1-g[3]),d-=r,n+=r,h-=r,S+=r),o[0]=d,o[1]=h,o[2]=n,o[3]=S,o}const pe=new Jt,z=U(),j=U(),P=Ce(),te=Vt(),he=U(),k=E(),M=E(),je=_t(),$=U(),ve=U(),Me=Ce(),mt=We(),B=E(),N=E(),H=E(),W=E(),me=E();class qa{constructor(){this.renderScreenPosition=U(),this.screenSize=E(),this.centerView=U(),this.pixelRatioTolerance=0,this.halfOutlineSize=0,this.rotationAngle=0,this.anchor=E()}}class Ga{constructor(){this.x=0,this.y=0,this.screenSize=E(),this.pixelRatioTolerance=0,this.halfOutlineSize=0,this.rotationAngle=0,this.anchor=E()}}const Be=new qa,He=new Ga,Le={normal:U(),cosAngle:0},ka=1,Na=2,y=Ye(0,0),Wa=Dt(0,0,1);class Ya extends ma{constructor(){super(...arguments),this.renderOccluded=1,this.testsTransparentRenderOrder=0,this.isDecoration=!1,this.color=$e,this.size=At,this.polygonOffset=0,this.anchorPosition=Ye(.5,.5),this.screenOffset=[0,0],this.shaderPolygonOffset=1e-5,this.textureIsSignedDistanceField=!1,this.sampleSignedDistanceFieldTexelCenter=!1,this.outlineColor=$e,this.outlineSize=0,this.distanceFieldBoundingBox=Ce(),this.rotation=0,this.hasRotation=!1,this.vvSizeEnabled=!1,this.vvSize=null,this.vvColor=null,this.vvOpacity=null,this.hasVertexColor=!1,this.hasVertexSize=!1,this.hasVertexRotation=!1,this.hasVertexUVi=!1,this.hasVertexCenterOffset=!1,this.hasSlicePlane=!1,this.pixelSnappingEnabled=!0,this.centerOffsetUnits="world",this.drawAsLabel=!1,this.depthEnabled=!0,this.focusAreaStyle=null,this.draped=!1,this.isLabel=!1}get hasVVSize(){return!!this.vvSize}get hasVVColor(){return!!this.vvColor}get hasVVOpacity(){return!!this.vvOpacity}}class Xa{constructor(e){this.baseInstanceLayout=pt,this.layout=ht(e)}elementCount(e){return e.get("position").indices.length}elementCountBaseInstance(e){return e.get("uv0").indices.length}write(e,t,a,r,s){if(s==null)return;const{buffer:l,offset:o}=s,{position:f,normal:d,color:n,size:u,rotation:h,centerOffset:S,groundDistance:g,featureAttribute:A,uvi:F}=l;ra(a.get("position"),e,f,o),sa(a.get("normal"),t,d,o);const w=a.get("position").indices.length;if(F){const x=a.get("uvi")?.data;if(x&&x.length>=4){const[p,b,V,R]=x;for(let D=0;D<w;++D){const c=o+D;F.setValues(c,p,b,V,R)}}}if(n&&oa(a.get("color"),4,n,o),u&&Ee(a.get("size"),u,o),h&&Fe(a.get("rotation"),h,o),S&&(a.get("centerOffset")?na(a.get("centerOffset"),S,o):de(S,o,w)),a.get("groundDistance")?Fe(a.get("groundDistance"),g,o):de(g,o,w),A&&(a.get("featureAttribute")?la(a.get("featureAttribute"),A,o):de(A,o,w)),r!=null){const x=a.get("position")?.indices;if(x){const p=x.length,b=l.getField("olidColor",Tt);ca(r,b,p,o)}}}writeBaseInstance(e,t){const{uv0:a}=t;Ee(e.get("uv0"),a,0)}}function qe(i,e,t){return i==null||e.vvSize==null?{scaleX:t,scaleY:t}:(ta(ve,e,i),{scaleX:ve[0]*t,scaleY:ve[1]*t})}function Qa(i,e){const t=e.computeRenderPixelSizeAtDist(i)*ft;return(e.aboveGround?1:-1)*t}function Ge(i,e,t,a,r){if(!i.verticalOffset?.screenLength){const f=ie(e);return r.update(t,f,i.screenSizePerspective,i.screenSizePerspectiveMinPixelReferenceSize,i.screenSizePerspectiveAlignment,null),0}const s=ie(e),l=i.screenSizePerspectiveAlignment??i.screenSizePerspective,o=aa(a,s,i.verticalOffset,t,l,i.screenSizePerspectiveMinPixelReferenceSize);return r.update(t,s,i.screenSizePerspective,i.screenSizePerspectiveMinPixelReferenceSize,i.screenSizePerspectiveAlignment,null),o}export{oe as L,Ia as V,ct as a,Fa as b,hi as c,ui as d,Ra as e,pi as f,Da as l,dt as n,nt as o,di as s};
