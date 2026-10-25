#!/usr/bin/env python3
"""F51 missing layout/history/output cells; reuse retained sources and binary."""
import argparse
import copy
import json
from pathlib import Path
import subprocess
from acceptance import Harness, at, sha


def verify(a):
    h = Harness(a.binary, a.workspace, 'agent-background-v4')
    root = Path(__file__).resolve().parents[1]
    source_project = 'b2101b49-480b-4fd8-a111-66b4d9a0bad1'
    source_revision = 'rev_534692f0d4089bc33fafac8da2881b2bba242ff5761ab88946581f4bf78ec0fa'
    original = h.inspect(source_project, source_revision)
    source_head = h.stage('source-head-before', lambda: h.inspect(source_project))
    complex_source = h.inspect(source_project, 'rev_7b145449be3c3736cba2b587bd27a0a1db67e665127d6ed524e4bba5b256e1d9')
    graphic = next(e for t in complex_source['project']['sequences'][0]['tracks'] for e in t['elements'] if e['content']['kind'] == 'graphic')
    layouts = []
    for name, width, height in [('landscape',320,180),('portrait',180,320),('square',240,240)]:
        project = 'agent-background-' + name + '-v4'
        def setup(project=project, name=name, width=width, height=height):
            h.request(name+'-copy', 'opencut_project_copy', {'source_project_id':source_project,'source_revision':source_revision,'project_id':project,'name':'F51 '+name,'idempotency_key':h.key(name+'-copy')})
            s = h.inspect(project)
            q = copy.deepcopy(s['project']['sequences'][0])
            main = next(t for t in q['tracks'] if t['id'] == q['main_track_id'])
            first = main['elements'][0]
            second = copy.deepcopy(first)
            second['id'] = 'f51-second-source'
            second['content']['asset_id'] = '6f2e16da-b308-489d-ab47-5aadf19a9858'
            second['timeline_range']['start'] = at(.5)
            main['elements'] = [first, second]
            translucent = copy.deepcopy(graphic)
            translucent.update(id='f51-transparent-content',name='F51 half opacity rectangle',channels=[],timeline_range={'start':at(0),'duration':at(1)})
            translucent['content']['graphic']['source_size'] = {'width':width,'height':height}
            translucent['content']['graphic']['fill'] = {'encoding':'srgb','red':200/255,'green':80/255,'blue':40/255,'alpha':1}
            translucent['visual'].update(opacity=.5,masks=[],effects=[],transform={'position_x_px':-.3*width,'position_y_px':0,'scale_x':.2,'scale_y':.2,'rotation_degrees':0})
            q['tracks'].append({'id':'f51-transparent-track','name':'F51 transparent content','kind':'graphic','enabled':True,'muted':False,'locked':False,'opacity':1,'gain':{'unit':'decibels','value':0},'elements':[translucent],'channels':[]})
            settings = copy.deepcopy(s['project']['settings'])
            settings['settings'].update(canvas_size={'width':width,'height':height},last_custom_canvas_size={'width':width,'height':height})
            receipt = h.apply(name+'-setup', {'project_id':project,'expected_revision':s['revision'],'idempotency_key':h.key(name+'-setup'),'label':'F51 two retained sources and transparent content','operations':[{'type':'settings_set','settings':settings},{'type':'sequence_put','sequence':q}]})
            snap = h.inspect(project)
            assert snap['revision'] == receipt['revision']
            return snap
        baseline = h.stage(name+'-setup', setup)
        layouts.append({'name':name,'width':width,'height':height,'project_id':project,'baseline':baseline})
    status = h.cli(['server','--directory',str(h.workspace/'service'),'status'])
    assert status['ready'] and sha(Path(status['executable']).read_bytes()) == h.candidate
    original_background = json.loads((h.workspace/'d2c-scene-stack-checkpoint.json').read_text())['jobs']['background']['job']
    retained_background = h.call('opencut_job_view', {'job_id':original_background['job_id']})
    assert retained_background['state'] == 'succeeded' and retained_background['artifacts'] == original_background['artifacts']
    h.artifact(retained_background)  # Integrity only; do not rerender/revalidate its accepted pixels.
    ui_input = a.report.with_name('background-ui-input.json')
    ui_report = a.report.with_name('background-ui.json')
    ui_input.write_text(json.dumps({'layouts':layouts},indent=2)+'\n')
    def ui():
        with a.report.with_name('background-ui.log').open('a') as log:
            p = subprocess.run(['node',str(root/'apps/web/scripts/verify-background.mjs'),'--workspace',str(h.workspace),'--endpoint',status['endpoint'],'--credential',status['token_file'],'--input',str(ui_input),'--report',str(ui_report)],cwd=root/'apps/web',stdout=log,stderr=subprocess.STDOUT)
        assert p.returncode == 0, str(a.report.with_name('background-ui.log'))
        return json.loads(ui_report.read_text())
    ui_result = h.stage('actual-layout-background-history-ui', ui)
    outputs = []
    for layout in ui_result['layouts']:
        name, project = layout['name'], layout['project_id']
        for kind in ['color','blur']:
            revision = layout[kind]['revision']
            request = {'project_id':project,'revision':revision,'sequence_id':layout['baseline']['project']['default_sequence_id'],'time':at(.25),'idempotency_key':h.key(name+'-'+kind+'-png')}
            stage = name+'-'+kind+'-png'
            if kind == 'blur' and name == 'landscape':
                # Added only after current lossy video probes exceeded8: compare
                # this exact fixed mixed revision losslessly to separate rendering
                # from encoding. Original qualified PNG remains original evidence.
                stage += '-video-diagnostic-v1'
                request['idempotency_key'] = h.key(stage)
            if stage == 'landscape-color-png' and stage in h.state['requests']:
                # A terminal failure is immutable. Preserve it and use an
                # explicit post-fix episode, never reset/retry its old key.
                original_request = h.state['requests'][stage]['input']
                original_job = h.call('opencut_job_scene_frame_png', original_request)
                failed = h.call('opencut_job_view', {'job_id':original_job['job_id']})
                assert failed['state'] == 'failed' and failed['failure']['code'] == 'VISUAL_BUDGET_EXCEEDED'
                h.state['original_admission_failure'] = failed
                h.save()
                stage += '-admission-fix-v1'
                request['idempotency_key'] = h.key(stage)
            job = h.stage(stage, lambda request=request,stage=stage:h.job(stage,'opencut_job_scene_frame_png',request))
            outputs.append({'layout':name,'kind':kind,'format':'png','job':job,'path':str(h.artifact(job))})
        request = {'project_id':project,'revision':layout['blur']['revision'],'sequence_id':layout['baseline']['project']['default_sequence_id'],'idempotency_key':h.key(name+'-blur-video'),'options':{'format':'mp4','quality':'high','include_audio':False}}
        job = h.stage(name+'-blur-video', lambda request=request,name=name:h.job(name+'-blur-video','opencut_job_export_video',request))
        outputs.append({'layout':name,'kind':'blur','format':'mp4','job':job,'path':str(h.artifact(job))})
    assert h.inspect(source_project) == source_head
    data = {'binary_sha256':h.candidate,'layouts':ui_result['layouts'],'outputs':outputs,'retained_background_checkpoint':str(h.workspace/'d2c-scene-stack-checkpoint.json'),'retained_background_artifact_integrity':True,'new_media_sources':0,'source_project_unchanged':True,'ui':ui_result,'service':{k:status[k] for k in ['endpoint','executable','instance','pid','ready']}}
    a.report.write_text(json.dumps(data,indent=2)+'\n')
    with a.report.with_name('background-oracle.log').open('a') as log:
        p = subprocess.run(['node',str(root/'apps/web/scripts/verify-background.mjs'),'--phase','oracle','--workspace',str(h.workspace),'--input',str(a.report),'--report',str(a.report.with_name('background-oracle.json'))],cwd=root/'apps/web',stdout=log,stderr=subprocess.STDOUT)
    assert p.returncode == 0, str(a.report.with_name('background-oracle.log'))
    data.update(status='passed_scoped_f51_background',oracle=json.loads(a.report.with_name('background-oracle.json').read_text()))
    a.report.write_text(json.dumps(data,indent=2)+'\n')
    print(json.dumps({'status':data['status'],'new_jobs':len(outputs),'new_media_sources':0}))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    for name in ['binary','workspace','report']:p.add_argument('--'+name,type=Path,required=True)
    verify(p.parse_args())
