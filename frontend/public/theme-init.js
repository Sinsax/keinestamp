// 主题初始化（head 内同步执行防 FOUC；独立文件避免 inline script 被 CSP script-src 'self' 拦截）
document.documentElement.dataset.theme = localStorage.getItem('keinestamp_theme') === 'night' ? 'night' : 'day'
