import {test, expect} from 'bun:test';
import {mkdtempSync, readFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {need, notApplicable} from '../../../scripts/coverage.mjs';

// `swift` leaves a `TemporaryDirectory.*` in TMPDIR each time it hands a run to the frontend: each run gets a TMPDIR of its own, removed after.
const xcrunSwift=(args,options={})=>{
  const tmp=mkdtempSync(resolve(tmpdir(),'exact-swift-'));
  try { return spawnSync('xcrun',['swift',...args],{encoding:'utf8',...options,env:{...(options.env??process.env),TMPDIR:tmp}}); }
  finally { rmSync(tmp,{recursive:true,force:true}); }
};
const swift=process.platform==='darwin' ? xcrunSwift(['--version']) : null;
const check=process.platform!=='darwin' ? notApplicable('the extracted Swift placement harness requires macOS and xcrun')
  : need('swift', swift.status===0 ? null : `xcrun swift does not work${process.env.DEVELOPER_DIR ? ` with DEVELOPER_DIR=${process.env.DEVELOPER_DIR}` : ''}: ${(swift.stderr||swift.error?.message||`exit ${swift.status}`).trim()}`);

// Execute each production capture loop's empty-child branch with Swift values.
// The remainder of the loop is a stand-in for a successful bitmap upload.
for (const host of ['Mac','IOS']) check(`${host} clears zero-sized and display-none captures`,()=>{
  const file=`host/apple/Sources/ExactSurfaces/Gpu${host}.swift`; // ExactSurfaces since 244ac708d (LLP 1047.001)
  const source=readFileSync(process.env.R7_APPLE_SOURCE ?? file,'utf8');
  const start=source.indexOf('        for (i, child) in children.enumerated() {');
  const end=source.indexOf('            let hidden = child.hiddenByHost',start);
  const branch=source.slice(start,end);
  const swift=`struct Rect { var width: Double; var height: Double }
enum BatchValue { case string(String); var string: String? { if case .string(let s) = self { return s }; return nil } } // ExactKit's style value, reduced
struct Child { var frame: Rect; var style: [String:BatchValue]; var props: [String:String] = [:] }
struct Entry { var id: UInt32 = 1 }
final class Module {
 var cleared: [UInt32] = []
 func child(_ id: UInt32,_ i: UInt32,_ name: String,_ x: Float,_ y: Float,_ w: Float,_ h: Float,_ pw: UInt32,_ ph: UInt32,_ bytes: UnsafePointer<UInt8>?,_ n: Int) -> UInt32 {
  precondition([x,y,w,h] == [0,0,0,0] && pw == 0 && ph == 0 && bytes == nil && n == 0)
  cleared.append(i); return 0
 }
}
let m=Module(), e=Entry()
let children=[Child(frame:Rect(width:20,height:20),style:[:]),
 Child(frame:Rect(width:0,height:0),style:[:]),Child(frame:Rect(width:20,height:20),style:["display":.string("none")])]
var captured=0
func capture() -> Bool {
${branch}
 captured += 1
 }
 return true
}
precondition(capture()); precondition(m.cleared == [1,2]); precondition(captured == 1)
`;
  const env={...process.env};delete env.SDKROOT;
  const r=xcrunSwift(['-'],{input:swift,env,timeout:60000});
  expect(r.status, r.stderr).toBe(0);
},65000);

for(const host of ['Mac','IOS']) check(`${host} hidden placement box is zero, not the kernel frame`,()=>{
  const source=readFileSync(process.env.R7_APPLE_AGENT ?? `host/apple/Sources/ExactKit/${host}/Agent${host}.swift`,'utf8');
  const start=source.indexOf('        if (v as? NodeView)?.placedAncestor?.placementHidden == true');
  expect(start).toBeGreaterThan(-1);
  const guard=source.slice(start,source.indexOf('\n',start));
  const swift=`struct Rect: Equatable {var width:Int;static let zero=Rect(width:0)}
class View {var bounds=Rect(width:100)}
class NodeView: View {var placedAncestor:NodeView?;var placementHidden=false}
func box(_ v:View, region:Rect?=nil)->Rect {${guard}\nreturn v.bounds}
let parent=NodeView(), child=NodeView();child.placedAncestor=parent
precondition(box(child).width==100);parent.placementHidden=true
precondition(box(child)==Rect.zero)
`;
  const env={...process.env};delete env.SDKROOT;
  const r=xcrunSwift(['-'],{input:swift,env,timeout:60000});
  expect(r.status,r.stderr).toBe(0);
},65000);
