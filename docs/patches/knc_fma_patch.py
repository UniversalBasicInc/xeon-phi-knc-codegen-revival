import sys, os
os.chdir(sys.argv[1])
# ---------- X86InstrCompiler.td
p="X86InstrCompiler.td"; s=open(p).read()
old132 = '''    let Constraints = "$dst = $a" in
    def KNC_VFMADD132PS : I<0x98, RawFrm, (outs VR512:$dst),
        (ins VR512:$a, VR512:$c, VR512:$b), "knc.vfmadd132ps\\t$b, $c, $dst",
        [(set VR512:$dst, (v16f32 (fma VR512:$a, VR512:$b, VR512:$c)))]>;
    let Constraints = "$dst = $a" in
    def KNC_VFMADD132PD : I<0x98, RawFrm, (outs VR512:$dst),
        (ins VR512:$a, VR512:$c, VR512:$b), "knc.vfmadd132pd\\t$b, $c, $dst",
        [(set VR512:$dst, (v8f64 (fma VR512:$a, VR512:$b, VR512:$c)))]>;
'''
new132 = '''    // Three FMA forms, operands (tied, vvvv, rm): 132: dst = o1*o3 + o2; 213: dst = o2*o1 + o3;
    // 231: dst = o2*o3 + o1. ISel emits 132; X86InstrInfo::commuteInstructionImpl switches forms so
    // the two-address pass can tie whichever source operand dies (saves a copy per accumulate FMA).
    let Constraints = "$dst = $a", isCommutable = 1 in
    def KNC_VFMADD132PS : I<0x98, RawFrm, (outs VR512:$dst),
        (ins VR512:$a, VR512:$c, VR512:$b), "knc.vfmadd132ps\\t$b, $c, $dst",
        [(set VR512:$dst, (v16f32 (fma VR512:$a, VR512:$b, VR512:$c)))]>;
    let Constraints = "$dst = $a", isCommutable = 1 in
    def KNC_VFMADD132PD : I<0x98, RawFrm, (outs VR512:$dst),
        (ins VR512:$a, VR512:$c, VR512:$b), "knc.vfmadd132pd\\t$b, $c, $dst",
        [(set VR512:$dst, (v8f64 (fma VR512:$a, VR512:$b, VR512:$c)))]>;
    let Constraints = "$dst = $a", isCommutable = 1 in {
    def KNC_VFMADD213PS : I<0xa8, RawFrm, (outs VR512:$dst),
        (ins VR512:$a, VR512:$b, VR512:$c), "knc.vfmadd213ps\\t$c, $b, $dst", []>;
    def KNC_VFMADD213PD : I<0xa8, RawFrm, (outs VR512:$dst),
        (ins VR512:$a, VR512:$b, VR512:$c), "knc.vfmadd213pd\\t$c, $b, $dst", []>;
    def KNC_VFMADD231PS : I<0xb8, RawFrm, (outs VR512:$dst),
        (ins VR512:$a, VR512:$b, VR512:$c), "knc.vfmadd231ps\\t$c, $b, $dst", []>;
    def KNC_VFMADD231PD : I<0xb8, RawFrm, (outs VR512:$dst),
        (ins VR512:$a, VR512:$b, VR512:$c), "knc.vfmadd231pd\\t$c, $b, $dst", []>;
    }
'''
assert s.count(old132)==1, "td anchor"; s=s.replace(old132,new132); open(p,"w").write(s); print("td ok")
# ---------- encoder
p="MCTargetDesc/X86MCCodeEmitter.cpp"; s=open(p).read()
old="  bool KNCFMA = Opcode == X86::KNC_VFMADD132PS || Opcode == X86::KNC_VFMADD132PD;\n"
new='''  bool KNCFMA = Opcode == X86::KNC_VFMADD132PS || Opcode == X86::KNC_VFMADD132PD ||
                Opcode == X86::KNC_VFMADD213PS || Opcode == X86::KNC_VFMADD213PD ||
                Opcode == X86::KNC_VFMADD231PS || Opcode == X86::KNC_VFMADD231PD;
'''
assert s.count(old)==1, "enc anchor 1"; s=s.replace(old,new)
old="  case X86::KNC_VFMADD132PD: KNCExtraOpcode = 0x98; KNCExtraMap = 2; KNCExtraW = true; break;\n"
new=old+'''  case X86::KNC_VFMADD213PS: KNCExtraOpcode = 0xa8; KNCExtraMap = 2; break;
  case X86::KNC_VFMADD213PD: KNCExtraOpcode = 0xa8; KNCExtraMap = 2; KNCExtraW = true; break;
  case X86::KNC_VFMADD231PS: KNCExtraOpcode = 0xb8; KNCExtraMap = 2; break;
  case X86::KNC_VFMADD231PD: KNCExtraOpcode = 0xb8; KNCExtraMap = 2; KNCExtraW = true; break;
'''
assert s.count(old)==1, "enc anchor 2"; s=s.replace(old,new); open(p,"w").write(s); print("encoder ok")
# ---------- InstrInfo
p="X86InstrInfo.cpp"; s=open(p).read()
old="  case X86::KNC_VFMADD132PS:\n  case X86::KNC_VFMADD132PD:\n    return KNCFPScheduling ? 4 : 0;\n"
new="  case X86::KNC_VFMADD132PS:\n  case X86::KNC_VFMADD132PD:\n  case X86::KNC_VFMADD213PS:\n  case X86::KNC_VFMADD213PD:\n  case X86::KNC_VFMADD231PS:\n  case X86::KNC_VFMADD231PD:\n    return KNCFPScheduling ? 4 : 0;\n"
assert s.count(old)==1, "lat anchor"; s=s.replace(old,new)
i = s.index("bool X86InstrInfo::findCommutedOpIndices(const MachineInstr &MI,")
j = s.index("  switch (MI.getOpcode()) {\n", i) + len("  switch (MI.getOpcode()) {\n")
ins = '''  case X86::KNC_VFMADD132PS: case X86::KNC_VFMADD132PD:
  case X86::KNC_VFMADD213PS: case X86::KNC_VFMADD213PD:
  case X86::KNC_VFMADD231PS: case X86::KNC_VFMADD231PD: {
    // Three source operands (1 tied to dst, 2 = vvvv, 3 = rm); any pair commutes with a form switch.
    unsigned Op1 = SrcOpIdx1, Op2 = SrcOpIdx2;
    if (Op1 == CommuteAnyOperandIndex && Op2 == CommuteAnyOperandIndex) { Op1 = 2; Op2 = 3; }
    else if (Op1 == CommuteAnyOperandIndex) Op1 = (Op2 == 2) ? 3 : 2;
    else if (Op2 == CommuteAnyOperandIndex) Op2 = (Op1 == 2) ? 3 : 2;
    if (Op1 < 1 || Op1 > 3 || Op2 < 1 || Op2 > 3 || Op1 == Op2)
      return false;
    SrcOpIdx1 = Op1; SrcOpIdx2 = Op2;
    return true;
  }
'''
s = s[:j] + ins + s[j:]
i = s.index("MachineInstr *X86InstrInfo::commuteInstructionImpl(MachineInstr &MI, bool NewMI,")
j = s.index("  switch (Opc) {\n", i) + len("  switch (Opc) {\n")
ins = '''  case X86::KNC_VFMADD132PS: case X86::KNC_VFMADD132PD:
  case X86::KNC_VFMADD213PS: case X86::KNC_VFMADD213PD:
  case X86::KNC_VFMADD231PS: case X86::KNC_VFMADD231PD: {
    // Operands: 0 dst, 1 tied, 2 vvvv, 3 rm. 132: o1*o3+o2  213: o2*o1+o3  231: o2*o3+o1.
    // Swapping two sources keeps the value when the form changes as tabulated here.
    bool PS = Opc == X86::KNC_VFMADD132PS || Opc == X86::KNC_VFMADD213PS || Opc == X86::KNC_VFMADD231PS;
    unsigned Form = (Opc == X86::KNC_VFMADD132PS || Opc == X86::KNC_VFMADD132PD) ? 132
                  : (Opc == X86::KNC_VFMADD213PS || Opc == X86::KNC_VFMADD213PD) ? 213 : 231;
    unsigned Lo = std::min(OpIdx1, OpIdx2), Hi = std::max(OpIdx1, OpIdx2);
    unsigned NewForm;
    if (Lo == 1 && Hi == 2)      NewForm = Form == 132 ? 231 : Form == 231 ? 132 : 213;
    else if (Lo == 1 && Hi == 3) NewForm = Form == 132 ? 132 : Form == 231 ? 213 : 231;
    else if (Lo == 2 && Hi == 3) NewForm = Form == 132 ? 213 : Form == 231 ? 231 : 132;
    else return nullptr;
    unsigned NewOpc = NewForm == 132 ? (PS ? X86::KNC_VFMADD132PS : X86::KNC_VFMADD132PD)
                    : NewForm == 213 ? (PS ? X86::KNC_VFMADD213PS : X86::KNC_VFMADD213PD)
                                     : (PS ? X86::KNC_VFMADD231PS : X86::KNC_VFMADD231PD);
    WorkingMI = CloneIfNew(MI);
    WorkingMI->setDesc(get(NewOpc));
    break;
  }
'''
s = s[:j] + ins + s[j:]
open(p,"w").write(s); print("instrinfo ok")
