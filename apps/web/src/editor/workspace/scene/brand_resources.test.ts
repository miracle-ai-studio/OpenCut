// @vitest-environment jsdom
import { expect, it } from "vitest";
import { validateBrandSvg, type BrandSource } from "./brand_resources";
function source():BrandSource {
  const svg='<svg role="img" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><title>Example</title><path d="M0 0H24V24H0Z"/></svg>';
  return {collection:{name:"Simple Icons",release:"16.35.0",commit:"ed9a5192b24ad2f169bf1009931da0df27d5a49a",source_url:"https://github.com/simple-icons/simple-icons",inventory_sha256:"a".repeat(64),collection_license:{identifier:"CC0-1.0",source_url:null,notice:"collection only",attribution:null},disclaimer:"No blanket brand license"},entry:{id:"simple-icons:16.35.0:example",name:"Example",slug:"example",keywords:[],color:"000000",source_url:"https://example.org",guidelines_url:null,svg_sha256:"a".repeat(64),svg_size:new TextEncoder().encode(svg).length,license_identifier:"CC0-1.0",license_url:"https://creativecommons.org/publicdomain/zero/1.0/",import_available:true,unavailable_reason:null},svg,license:{identifier:"CC0-1.0",source_url:null,notice:"full notice",attribution:"provided attribution"},raster_width:512,raster_height:512};
}
it("admits the fixed inert SVG subset without changing source bytes",()=>{const s=source();expect(new TextDecoder().decode(validateBrandSvg(s,s.entry))).toBe(s.svg);});
it("rejects external references, scripts, CSS, nested elements and entity expansion before image decoding",()=>{
  for(const attack of ['<script>alert(1)</script>','<image href="https://example.org/x.png"/>','<path d="M0 0" onload="attack()"/>','<style>path{fill:url(https://example.org)}</style>','<g><path d="M0 0"/></g>']){
    const s=source();s.svg=s.svg.replace('</svg>',attack+'</svg>');s.entry.svg_size=new TextEncoder().encode(s.svg).length;expect(()=>validateBrandSvg(s,s.entry)).toThrow();
  }
  const s=source();s.svg='<!DOCTYPE svg [<!ENTITY x SYSTEM "file:///private">]>'+s.svg;s.entry.svg_size=new TextEncoder().encode(s.svg).length;expect(()=>validateBrandSvg(s,s.entry)).toThrow();
});
it("rejects another selected icon, release, availability, incomplete license and raster envelope",()=>{
  const entry=source().entry;
  for(const mutate of [(s:BrandSource)=>{s.entry.id="simple-icons:16.35.0:other";},(s:BrandSource)=>{s.collection.commit="b".repeat(40);},(s:BrandSource)=>{s.entry.import_available=false;},(s:BrandSource)=>{s.license.notice=null;},(s:BrandSource)=>{s.raster_width=513;}]){const s=source();mutate(s);expect(()=>validateBrandSvg(s,entry)).toThrow();}
});
