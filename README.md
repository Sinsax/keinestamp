# Keinestamp

Blender 椤圭洰杩涘害鐪嬫澘 鈥?鍗曚簩杩涘埗 CLI + Tauri GUI锛屽悗绔熀浜?Epic Games **Lore**锛?.8.6锛屽垎鍧楀幓閲?VCS锛夈€?
## 鍔熻兘

- **鏈湴璁板綍**: 涓€閿褰?Blender 椤圭洰杩涘害锛堟敮鎸佸鏂囦欢鍕鹃€夈€佹爣绛俱€?D 棰勮锛?- **鏃堕棿绾?*: 璁板綍娴忚銆佹枃浠跺彉鏇翠笌 GLB 3D 棰勮
- **鍥為€€/鎭㈠/椤舵浛**: 鍥為€€鍒颁换鎰忚褰曪紙`branch reset`锛夛紝鍙挙閿€鍥為€€锛屽洖閫€鍚庤褰曞嵆椤舵浛
- **闅愯棌鍒楄〃**: 鏈湴闅愯棌璺緞锛坅pp-config 鎸佷箙鍖栵級锛宍web/` 涓?`.keinestamp.toml` 棰勮涓嶅彲绉婚櫎
- **鍘婚噸瀛樺偍**: Lore 鍐呭瀵诲潃鍘婚噸锛?blend 杩炲瓨 10 娆′粨搴撳閲?<1%锛堝疄娴?99.9%锛?- **Web 閮ㄧ讲**: 閮ㄧ讲鍒板绉熸埛 web 鏈嶅姟鍣紙web 浜х墿杩涗富浠撳簱 `web/`锛宬ind=web 鎻愪氦锛夛紝娴忚鍣ㄧ櫥褰曟煡鐪嬫椂闂寸嚎
- **灞€鍩熺綉澶囦唤**: `serve` 鎷夎捣 loreserver锛屽浠芥寜閽?= `lore push`

## 鍓嶇疆

- **Lore v0.8.6锛堝墠缃緷璧栵紝鏋勫缓鏃惰嚜鍔ㄦ崋缁戯級**锛歚build.rs` 鎸夊钩鍙拌嚜鍔ㄤ笅杞?  `lore` + `loreserver` 浜岃繘鍒讹紙GitHub releases v0.8.6锛宻ha256 鏍￠獙锛岀己澶?鏍￠獙澶辫触 鈫?鏋勫缓澶辫触锛夛紝
  鎹嗙粦浜?`src-tauri/resources/lore-bin/`锛屽苟**鑷姩鎷疯礉鍒版瀯寤轰骇鐗╃洰褰?*锛坄target/<profile>/lore-bin`锛屼笌 exe 鍚岀洰褰曪級
- **杩愯 = 涓や欢濂?*锛歚keinestamp.exe` + 鍚岀洰褰?`lore-bin/`锛坙ore.exe + loreserver.exe + LICENSE锛夆€斺€?  zip 鍒嗗彂鏃惰В鍘嬪埌鍚屼竴鐩綍鍗冲彲鐩存帴杩愯锛屼笉渚濊禆绯荤粺瀹夎浠讳綍涓滆タ
- 杩愯鏃舵煡鎵鹃『搴忥細`KEINESTAMP_LORE_BIN` 鐜鍙橀噺 鈫?exe 鍚岀洰褰?`lore-bin/` 鈫?Tauri 瀹夎鍖呰祫婧?鈫?骞冲彴榛樿璺緞
- 鍚姩妫€娴嬶細CLI/serve/GUI 鍧囨牎楠?`lore --version` 绮剧‘ == 0.8.6锛屼笉绗︽姤閿欙紙GUI 寮圭獥 + 鏍稿績鎿嶄綔绂佺敤锛?- Lore 涓?MIT 璁稿彲锛埪?Epic Games, Inc.锛夛紝鎹嗙粦鍒嗗彂闄勫甫 LICENSE 澹版槑
- 鍛戒护褰㈡€侀€熸煡琛細`docs/lore-commands.md`锛堝疄娴嬭褰曪級

## CLI 鍛戒护

```
keinestamp init <path>          鍒濆鍖栭」鐩紙lore 寤轰粨 + 闅愯棌鍒楄〃锛?keinestamp record <message>     璁板綍杩涘害
keinestamp status               鏌ョ湅鍙樻洿锛堣繃婊ら殣钘忥級
keinestamp log                  鏌ョ湅鏃堕棿绾匡紙杩囨护 kind=web锛?keinestamp rollback <entry>     鍥為€€鍒拌褰?keinestamp restore              鎾ら攢鍥為€€
keinestamp export <entry> <dir> 瀵煎嚭璁板綍鐗堟湰
keinestamp diff <entry> <file>  鏌ョ湅鏂囦欢宸紓
keinestamp push|pull            鎺ㄩ€佸埌 loreserver / 鍚屾
keinestamp serve [--port 7799]  鍚姩 web 鏈嶅姟鍣紙鑷姩鎷夎捣 loreserver锛?```

## 寮€鍙?
```powershell
cargo test              # 鍗曞厓 + 闆嗘垚娴嬭瘯锛堥渶 loreserver 鍦ㄧ嚎锛?cargo tauri dev         # GUI 寮€鍙戯紙frontend: Svelte 5锛?cd src-tauri; cargo build
cd frontend; npm run check
```

## 鏋舵瀯

```
GUI (Tauri) 鈹€鈹€ lore_project 鈹€鈹€ lore_backend锛堝敮涓€ lore 璋冪敤鐐癸級鈹€鈹€ loreserver
                     鈹溾攢鈹€ hidden锛堥殣钘忓垪琛級     鈹斺攢鈹€ web/锛坘ind=web 鎻愪氦锛?                     鈹斺攢鈹€ timeline锛坵eb/timeline.json 灞曠ず缂撳瓨锛?serve 鈹€鈹€ ensure_loreserver + 澶氱鎴?HTTP
```

璇︾粏绾︽潫瑙?`AGENTS.md`锛涘疄鏂借鍒掕 `docs/superpowers/plans/2026-08-10-keinestamp-lore-v2.md`銆?
## Linux 鏈嶅姟鍣ㄩ儴缃?
`deploy/keinestamp.service`锛坰ystemd锛夌敤浜?web 鏈嶅姟鍣紙serve锛夛細

```bash
sudo cp deploy/keinestamp.service /etc/systemd/system/
# 淇敼 User/Group 涓庤矾寰?sudo systemctl daemon-reload
sudo systemctl enable --now keinestamp
```
