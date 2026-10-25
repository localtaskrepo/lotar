// Small progressive enhancements: install tabs, copy buttons and the latest
// release number. The page works without any of them.

for (const tabs of document.querySelectorAll('[data-tabs]')) {
    const buttons = [...tabs.querySelectorAll('[role="tab"]')]
    const select = (button) => {
        for (const other of buttons) {
            const selected = other === button
            other.setAttribute('aria-selected', String(selected))
            other.tabIndex = selected ? 0 : -1
            document.getElementById(other.getAttribute('aria-controls')).hidden = !selected
        }
    }
    buttons.forEach((button, index) => {
        button.addEventListener('click', () => select(button))
        button.addEventListener('keydown', (event) => {
            const step = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0
            if (!step) return
            event.preventDefault()
            const next = buttons[(index + step + buttons.length) % buttons.length]
            select(next)
            next.focus()
        })
    })
    // Windows visitors see the Scoop command first.
    if (/Windows/i.test(navigator.userAgent)) {
        const windows = buttons.find((button) => button.id === 'tab-scoop')
        if (windows) select(windows)
    }
}

for (const button of document.querySelectorAll('[data-copy]')) {
    button.addEventListener('click', async () => {
        const text = button.parentElement.querySelector('code').textContent.trim()
        try {
            await navigator.clipboard.writeText(text)
            button.textContent = 'Copied'
        } catch {
            button.textContent = 'Press Ctrl+C'
        }
        setTimeout(() => (button.textContent = 'Copy'), 1600)
    })
}

const versionTag = document.querySelector('[data-latest-version]')
if (versionTag) {
    fetch('https://api.github.com/repos/localtaskrepo/lotar/releases/latest', {
        headers: { Accept: 'application/vnd.github+json' },
    })
        .then((response) => (response.ok ? response.json() : null))
        .then((release) => {
            if (!release?.tag_name) return
            versionTag.textContent = `Latest ${release.tag_name}`
            versionTag.hidden = false
        })
        .catch(() => {})
}
