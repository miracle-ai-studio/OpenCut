#!/usr/bin/env python3
"""Read-only CLI/MCP equality probe for the synthetic browser FPS fixture."""
import importlib.util,json,subprocess,sys
from pathlib import Path
spec=importlib.util.spec_from_file_location('editorial',Path(__file__).with_name('verify-editorial.py'))
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
binary,database,root,request=sys.argv[1:];request=json.loads(request)
reply=subprocess.run([binary,'--json','--database',database,'call','opencut_media_import_plan_v1','--input','-'],input=json.dumps(request),text=True,capture_output=True,check=True)
cli=json.loads(reply.stdout);assert cli['error'] is None
mcp=module.Mcp(Path(binary),Path(database),Path(root))
try: other=mcp.tool('opencut_media_import_plan_v1',request)
finally:mcp.close()
permissions={}
for principal,actions in [('fps-reader',['read']),('fps-editor',['read','edit'])]:
    policy={'principal_id':principal,'expected_revision':'0','enabled':True,'grants':[{'project_id':request['project_id'],'actions':actions}]}
    result=subprocess.run([binary,'--database',database,'--json','access','set-policy','--input','-'],input=json.dumps(policy),text=True,capture_output=True,check=True)
    credential=Path(root)/(principal+'.credential')
    subprocess.run([binary,'--database',database,'--json','access','issue',principal,'--output',str(credential)],capture_output=True,text=True,check=True)
    scoped=subprocess.run([binary,'--database',database,'--credential-file',str(credential),'--json','call','opencut_media_import_plan_v1','--input','-'],input=json.dumps(request),text=True,capture_output=True)
    reply=json.loads(scoped.stdout)
    if principal=='fps-reader':assert scoped.returncode and reply['error']['code']=='NOT_AUTHORIZED'
    else:assert not scoped.returncode and reply['data']==cli['data']
    permissions[principal]=reply
print(json.dumps({'cli':cli['data'],'mcp':other,'permissions':permissions}))
