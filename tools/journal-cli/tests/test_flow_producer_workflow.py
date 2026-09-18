#!/usr/bin/python3
from __future__ import annotations
import hashlib, json, os
from pathlib import Path
import shutil, subprocess, tempfile, unittest

from journal_cli.flow import producer_workflow as producer
from journal_cli.flow.workflow_common import load_canonical_json, validate_review_package


def git(repo: Path, *args: str, ok: int = 0) -> str:
    result=subprocess.run(["/usr/bin/git","-C",str(repo),*args],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
    if result.returncode!=ok: raise AssertionError((args,result.returncode,result.stderr))
    return result.stdout.strip()


class ProducerTests(unittest.TestCase):
    def setUp(self):
        self.temp=Path(tempfile.mkdtemp(prefix="producer-test-")); os.chmod(self.temp,0o700)
        self.repo=self.temp/"repo"; subprocess.run(["/usr/bin/git","init","-q",self.repo],check=True)
        git(self.repo,"config","user.name","Test"); git(self.repo,"config","user.email","test@example.invalid")
        (self.repo/"a").write_text("a"); git(self.repo,"add","a"); git(self.repo,"commit","-qm","base"); self.base=git(self.repo,"rev-parse","HEAD")
        (self.repo/"a").write_text("aa"); git(self.repo,"commit","-qam","one"); self.predecessor=git(self.repo,"rev-parse","HEAD")
        (self.repo/"b").write_text("b"); git(self.repo,"add","b"); git(self.repo,"commit","-qm","two"); self.head=git(self.repo,"rev-parse","HEAD")
        self.launcher=self.temp/"launcher"; self._source_env={key:os.environ.get(key) for key in ("SYNC_SOURCE_FLOW_LAUNCHER","SYNC_SOURCE_FLOW_LAUNCHER_SHA256")}; self._write_launcher("verified")
        self.evidence=self.temp/"TESTS.md"; self.evidence.write_text("tests pass\n")
    def tearDown(self):
        for key,value in self._source_env.items():
            if value is None: os.environ.pop(key,None)
            else: os.environ[key]=value
        shutil.rmtree(self.temp)
    def _write_launcher(self,mode: str):
        script=f'''#!/usr/bin/python3
import base64,json,pathlib,sys
args=sys.argv[1:]; command=args[0]; log=pathlib.Path(__file__).with_name("dispatch.log"); log.open("a").write(command+"\\n")
mode_file=pathlib.Path(__file__).with_name("ready-mode"); selected=mode_file.read_text().strip() if mode_file.exists() else "{mode}"
def value(name): return args[args.index(name)+1]
def emit(value_): print(json.dumps(value_,sort_keys=True,separators=(",",":"))+"\\n",end="")
if command=="publish":
 inputs={{"classification":"nonnormative-operation-local","expectedDescriptorBytes":375,"expectedDescriptorSha256":"1"*64,"endpoint":"http://127.0.0.1:8192/interface","route":[],"owner":"rocky","portable":False,"readyMarkerPath":["source","p","releases","r","release.scm"]}}
 receipt={{"operationId":"0"*32,"operationLocalReadyInputs":inputs,"outcome":"ready-marker-write-accepted","releaseId":value("--release-id")}}
 pathlib.Path(value("--receipt")).write_text(json.dumps(receipt,sort_keys=True,separators=(",",":"))+"\\n")
 emit({{"operationLocalReadyInputs":inputs,"outcome":"ready-marker-write-accepted"}})
elif command=="ready":
 fixed={{"descriptorBytes":375,"descriptorPath":["source","p","releases","r","release.scm"],"descriptorSha256":"1"*64,"endpoint":"http://127.0.0.1:8192/interface","index":7,"owner":"rocky"}}
 receipt={{"aggregateBytes":100,"chunks":1,"entries":5,"operationId":"1"*32,"outcome":selected}}; receipt.update({{"fixedReference":fixed}} if selected=="verified" else {{}})
 pathlib.Path(value("--receipt")).write_text(json.dumps(receipt,sort_keys=True,separators=(",",":"))+"\\n")
 if selected=="verified": emit({{"fixedReference":fixed,"outcome":"verified","reference":base64.b64encode(b"(source-fixed-v2)\\n").decode(),"treeSha256":"2"*64}})
 else: emit({{"outcome":"not-ready"}}); raise SystemExit(1)
else: raise SystemExit(2)
'''
        self.launcher.write_text(script); self.launcher.chmod(0o700)
        os.environ["SYNC_SOURCE_FLOW_LAUNCHER"]=str(self.launcher); os.environ["SYNC_SOURCE_FLOW_LAUNCHER_SHA256"]=hashlib.sha256(self.launcher.read_bytes()).hexdigest()
    def plan(self,kind="full",changed=None):
        base=self.base if kind=="full" else self.predecessor
        return {"schema":producer.PLAN_SCHEMA,
          "git":{"allowedChangedPaths":changed or (["a","b"] if kind=="full" else ["b"]),"base":base,"head":self.head,"kind":kind,"prerequisite":base,"repo":str(self.repo),"repositoryId":"sync-web"},
          "evidence":[{"packagePath":"TESTS.md","source":str(self.evidence)}],
          "reviewers":[{"principal":["galactica","grace","*state*","grace"]}],
          "source":{"endpoint":"http://127.0.0.1:8192/interface","launcher":{"path":str(self.launcher),"sha256":hashlib.sha256(self.launcher.read_bytes()).hexdigest()},
             "owner":"rocky","projectLabel":"project","releaseLabel":"release","route":[],"settleSeconds":0}}
    def write_plan(self,value=None,name="plan.json"):
        path=self.temp/name; path.write_bytes(producer.canonical_json(value or self.plan())); return path
    def test_full_and_correction_match_frozen_package(self):
        full_op=self.temp/"full"; result=producer.execute(self.write_plan(),full_op,dry_run=True)
        review,observed=validate_review_package(full_op/"package")
        self.assertEqual(result["outcome"],"preflight-verified"); self.assertEqual(review["git"]["head"],self.head)
        self.assertEqual(review["git"]["prerequisite"],self.base); self.assertEqual(set(observed),{"REVIEW.json","candidate.bundle","CHANGED-FILES.txt","COMMITS.txt","TESTS.md"})
        correction_op=self.temp/"correction"; correction=producer.execute(self.write_plan(self.plan("correction"),"correction-plan.json"),correction_op,dry_run=True)
        self.assertEqual(correction["review"]["git"]["commitCount"],1); self.assertEqual(correction["review"]["git"]["prerequisite"],self.predecessor)
        self.assertLess((correction_op/"package/candidate.bundle").stat().st_size,(full_op/"package/candidate.bundle").stat().st_size)
    def test_scope_and_dirty_fail_before_external_dispatch(self):
        wrong=self.write_plan(self.plan(changed=["a"]),"wrong.json")
        with self.assertRaisesRegex(Exception,"allowed changed paths"):
            producer.execute(wrong,self.temp/"wrong",dry_run=False)
        self.assertFalse((self.temp/"dispatch.log").exists())
        (self.repo/"dirty").write_text("x")
        with self.assertRaisesRegex(Exception,"not clean"):
            producer.execute(self.write_plan(name="dirty-plan.json"),self.temp/"dirty-op",dry_run=False)
        self.assertFalse((self.temp/"dispatch.log").exists())
    def test_executable_git_config_is_rejected_without_execution(self):
        marker=self.temp/"marker"
        for index,(key,value) in enumerate((("core.fsmonitor",f"touch {marker}"),("diff.external",f"touch {marker}"),("filter.bad.clean",f"touch {marker}"),("remote.origin.promisor","true"))):
            git(self.repo,"config",key,value)
            with self.assertRaisesRegex(Exception,"executable/network"):
                producer.execute(self.write_plan(name=f"hostile-{index}.json"),self.temp/f"hostile-{index}",dry_run=True)
            self.assertFalse(marker.exists()); git(self.repo,"config","--unset-all",key)
    def test_inherited_git_control_environment_is_removed(self):
        other=self.temp/"other.git"; subprocess.run(["/usr/bin/git","init","--bare","-q",other],check=True)
        marker=self.temp/"env-marker"
        hostile={"GIT_DIR":str(other),"GIT_WORK_TREE":str(self.temp/"wrong-tree"),"GIT_CONFIG_COUNT":"1","GIT_CONFIG_KEY_0":"core.fsmonitor","GIT_CONFIG_VALUE_0":f"touch {marker}","GIT_ALTERNATE_OBJECT_DIRECTORIES":str(other/"objects")}
        original={key:os.environ.get(key) for key in hostile}
        try:
            os.environ.update(hostile)
            result=producer.execute(self.write_plan(name="hostile-env.json"),self.temp/"hostile-env",dry_run=True)
        finally:
            for key,value in original.items():
                if value is None: os.environ.pop(key,None)
                else: os.environ[key]=value
        self.assertEqual(result["outcome"],"preflight-verified"); self.assertFalse(marker.exists())
    def test_symlink_and_gitlink_are_rejected(self):
        os.symlink("a",self.repo/"link"); git(self.repo,"add","link"); git(self.repo,"commit","-qm","symlink"); symlink_head=git(self.repo,"rev-parse","HEAD")
        plan=self.plan(); plan["git"]["head"]=symlink_head; plan["git"]["allowedChangedPaths"]=["a","b","link"]
        with self.assertRaisesRegex(Exception,"symlink/gitlink"):
            producer.execute(self.write_plan(plan,"symlink.json"),self.temp/"symlink",dry_run=True)
        git(self.repo,"reset","--hard",self.head)
        subprocess.run(["/usr/bin/git","clone","-q",str(self.repo),str(self.repo/"sub")],check=True)
        git(self.repo/"sub","checkout","-q",self.base)
        git(self.repo,"add","sub"); git(self.repo,"commit","-qm","gitlink"); gitlink_head=git(self.repo,"rev-parse","HEAD")
        plan=self.plan(); plan["git"]["head"]=gitlink_head; plan["git"]["allowedChangedPaths"]=["a","b","sub"]
        with self.assertRaisesRegex(Exception,"symlink/gitlink"):
            producer.execute(self.write_plan(plan,"gitlink.json"),self.temp/"gitlink",dry_run=True)
    def test_bundle_advertisement_is_exact(self):
        op=self.temp/"bundle"; producer.execute(self.write_plan(),op,dry_run=True)
        prerequisites,heads=producer._bundle_header(op/"package/candidate.bundle")
        self.assertEqual(prerequisites,[self.base]); self.assertEqual(heads,[(self.head,"refs/sync-source-flow/candidate")])
        hostile=self.temp/"hostile.bundle"; hostile.write_bytes(b"# v2 git bundle\n"+self.head.encode()+b" refs/one\n"+self.head.encode()+b" refs/two\n\nPACK")
        prerequisites,heads=producer._bundle_header(hostile); self.assertEqual(len(heads),2)
    def test_publish_uses_exact_ready_fields_and_writes_handoff(self):
        op=self.temp/"publish"; result=producer.execute(self.write_plan(),op,dry_run=False)
        self.assertEqual(result["outcome"],"ready"); self.assertEqual((self.temp/"dispatch.log").read_text().splitlines(),["publish","ready"])
        handoff=load_canonical_json(op/"handoff.json"); self.assertFalse(handoff["states"]["executed"]); self.assertEqual(handoff["outcome"],"verified"); self.assertEqual(handoff["fixedReference"]["index"],7)
        ready_command=(op/"source-evidence/ready.stdout.json").read_text(); self.assertIn('"outcome":"verified"',ready_command)
        self.assertTrue((op/"fixed-reference.scm").exists())
    def test_not_ready_stops_once_with_exact_resume_material(self):
        self._write_launcher("not-ready"); plan=self.plan(); plan_path=self.write_plan(plan,"not-ready-plan.json")
        op=self.temp/"not-ready"; result=producer.execute(plan_path,op,dry_run=False)
        self.assertEqual(result["outcome"],"not-ready"); self.assertEqual((self.temp/"dispatch.log").read_text().splitlines(),["publish","ready"])
        resume=load_canonical_json(op/"ready-resume.json"); self.assertEqual(resume["readyInputs"]["expectedDescriptorBytes"],375)
        self.assertEqual(resume["readyInputs"]["expectedDescriptorSha256"],"1"*64); self.assertFalse((op/"fixed-reference.scm").exists())
        tampered=dict(resume); tampered["launcher"]={"path":"/definitely/absent/source-launcher","sha256":"0"*64}
        (op/"tampered-absent.json").write_bytes(producer.canonical_json(tampered))
        with self.assertRaises(Exception): producer.execute_ready(op/"tampered-absent.json",self.temp/"tampered-absent-op")
        self.assertFalse((self.temp/"tampered-absent-op").exists())
        alternate=self.temp/"alternate-launcher"; alternate.write_bytes(self.launcher.read_bytes()); alternate.chmod(0o700)
        tampered=dict(resume); tampered["launcher"]={"path":str(alternate),"sha256":hashlib.sha256(alternate.read_bytes()).hexdigest()}
        (op/"tampered-unequal.json").write_bytes(producer.canonical_json(tampered))
        with self.assertRaisesRegex(Exception,"installed.*resume"): producer.execute_ready(op/"tampered-unequal.json",self.temp/"tampered-unequal-op")
        self.assertFalse((self.temp/"tampered-unequal-op").exists())
        self.assertEqual((self.temp/"dispatch.log").read_text().splitlines(),["publish","ready"])
        (self.temp/"ready-mode").write_text("verified")
        fresh=self.temp/"fresh-ready"; verified=producer.execute_ready(op/"ready-resume.json",fresh)
        self.assertEqual(verified["outcome"],"verified"); self.assertEqual((self.temp/"dispatch.log").read_text().splitlines(),["publish","ready","ready"])
        self.assertTrue((fresh/"fixed-reference.scm").exists())
    def test_plan_rejects_noncanonical_overlap_and_invalid_route(self):
        plan=self.plan(); plan["evidence"][0]["packagePath"]="REVIEW.json"
        with self.assertRaisesRegex(Exception,"collides"): producer.validate_plan(plan)
        plan=self.plan(); plan["source"]["route"]=["../x"]
        with self.assertRaisesRegex(Exception,"route"): producer.validate_plan(plan)

if __name__=="__main__": unittest.main()
