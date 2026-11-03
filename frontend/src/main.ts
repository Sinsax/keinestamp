import { mount } from 'svelte'
import './app.scss'
import App from './App.svelte'
import * as api from './api'
import { setLocale, syncSystemLocale, type Locale } from './lib/i18n/index.svelte'

// 语言在挂载前定稿（Rust 侧已按 env → AppConfig → 系统语言 解析好），
// 避免英文系统用户看到一帧中文界面。失败则保持默认中文。
api
  .getEffectiveLocale()
  .then((info) => {
    syncSystemLocale(info.system_tag as Locale)
    setLocale(info.tag as Locale)
  })
  .catch(() => { /* 保持默认 zh-CN */ })
  .finally(() => {
    const app = mount(App, {
      target: document.getElementById('app')!,
    })

    void app
  })
