#!/usr/bin/env python3
"""Offline verifier for vendored Windows 11 x64 export baselines."""
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
BASE = {
 "kernel32": ("dlls/kernel32/exports/windows-11-x64-exports.tsv", "dlls/kernel32/exports/kernel32-x64.def"),
 "kernelbase": ("dlls/kernelbase/exports/windows-11-x64-exports.tsv", "dlls/kernelbase/exports/kernelbase-x64.def"),
 "shell32": ("dlls/shell32/exports/windows-11-x64-exports.tsv", "dlls/shell32/exports/shell32-x64.def"),
}
def load(path):
 records=[]
 for no,line in enumerate((ROOT/path).read_text(encoding="utf-8").splitlines(),1):
  if not line or line.startswith("#"): continue
  cols=line.split("\t")
  if len(cols)!=3: raise SystemExit(f"{path}:{no}: expected ordinal<TAB>name<TAB>forwarder")
  try: ordinal=int(cols[0])
  except ValueError: raise SystemExit(f"{path}:{no}: invalid ordinal {cols[0]!r}")
  records.append((ordinal,cols[1],cols[2]))
 if len({r[0] for r in records})!=len(records): raise SystemExit(f"{path}: duplicate ordinals")
 names=[r[1] for r in records if r[1]]
 if len(set(names))!=len(names): raise SystemExit(f"{path}: duplicate export names")
 return records
def expected(dll,records):
 out=[f"LIBRARY {dll}.dll","EXPORTS"]
 for ordinal,name,target in records:
  symbol=name or f"CognacOrdinalStub_{ordinal}"
  if target: out.append(f"    {symbol}={target} @{ordinal}"+(" NONAME" if not name else ""))
  elif not name: out.append(f"    {symbol} @{ordinal} NONAME")
  else: out.append(f"    {name} @{ordinal}")
 return "\n".join(out)+"\n"
def main():
 all_records={}
 for dll,(tsv,definition) in BASE.items():
  records=load(tsv); all_records[dll]=records
  actual=(ROOT/definition).read_text(encoding="utf-8")
  if actual!=expected(dll,records): raise SystemExit(f"{definition}: does not exactly match {tsv}")
  fw=sum(bool(x[2]) for x in records); ordinal_only=sum(not x[1] for x in records)
  print(f"{dll}: {len(records)} entries; {fw} forwarders; {ordinal_only} ordinal-only; .def exact")
 kb_names={name for _,name,_ in all_records["kernelbase"] if name}
 nt_names={line.strip() for line in (ROOT/"ntdll/exports/windows-11-x64-exports.txt").read_text().splitlines() if line.strip() and not line.lstrip().startswith("#")}
 k32=all_records["kernel32"]
 kb_fw=[(name,target) for _,name,target in k32 if target.upper().startswith("KERNELBASE.")]
 nt_fw=[(name,target) for _,name,target in k32 if target.upper().startswith("NTDLL.")]
 missing_kb=[(name,target) for name,target in kb_fw if target.split(".",1)[1] not in kb_names]
 missing_nt=[(name,target) for name,target in nt_fw if target.split(".",1)[1] not in nt_names]
 if missing_kb: raise SystemExit(f"kernel32 KERNELBASE targets missing: {missing_kb[:20]}")
 if missing_nt: raise SystemExit(f"kernel32 NTDLL targets missing: {missing_nt[:20]}")
 print(f"kernel32 forwarders: {len(kb_fw)} to KERNELBASE (all targets exist), {len(nt_fw)} to NTDLL (all targets exist); API-set forwarders retained verbatim")
 print("All export manifests are local; no network access is used.")
if __name__=="__main__": main()
