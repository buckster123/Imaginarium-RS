const TOKEN_KEY = 'imaginarium_token'

export function getToken() {
  return sessionStorage.getItem(TOKEN_KEY) || ''
}

export function setToken(t) {
  if (t) sessionStorage.setItem(TOKEN_KEY, t)
  else sessionStorage.removeItem(TOKEN_KEY)
}

export function clearToken() {
  sessionStorage.removeItem(TOKEN_KEY)
}

async function request(path, opts = {}) {
  const headers = { ...(opts.headers || {}) }
  const token = getToken()
  if (token) headers['Authorization'] = `Bearer ${token}`
  if (opts.body && !(opts.body instanceof FormData)) {
    headers['Content-Type'] = 'application/json'
  }
  const res = await fetch(path, { ...opts, headers })
  const text = await res.text()
  let data
  try {
    data = text ? JSON.parse(text) : null
  } catch {
    data = { raw: text }
  }
  if (!res.ok) {
    const msg = data?.error || data?.message || res.statusText || `HTTP ${res.status}`
    const err = new Error(msg)
    err.status = res.status
    err.data = data
    throw err
  }
  return data
}

export const api = {
  health: () => request('/health'),
  models: () => request('/v1/models'),
  estimate: (body) => request('/v1/estimate', { method: 'POST', body: JSON.stringify(body) }),
  imageGen: (body) =>
    request('/v1/images/generations', { method: 'POST', body: JSON.stringify(body) }),
  imageEdit: (body) =>
    request('/v1/images/edits', { method: 'POST', body: JSON.stringify(body) }),
  videoGen: (body) =>
    request('/v1/videos/generations', { method: 'POST', body: JSON.stringify(body) }),
  videoEdit: (body) =>
    request('/v1/videos/edits', { method: 'POST', body: JSON.stringify(body) }),
  videoExtend: (body) =>
    request('/v1/videos/extensions', { method: 'POST', body: JSON.stringify(body) }),
  jobs: (limit = 30) => request(`/v1/jobs?limit=${limit}`),
  job: (id) => request(`/v1/jobs/${encodeURIComponent(id)}`),
  jobWait: (id) => request(`/v1/jobs/${encodeURIComponent(id)}/wait`, { method: 'POST', body: '{}' }),
  libraryContentUrl: (id) => `/v1/library/${encodeURIComponent(id)}/content`,
  tokensList: () => request('/v1/tokens'),
  tokenCreate: (body) => request('/v1/tokens', { method: 'POST', body: JSON.stringify(body) }),
  tokenRevoke: (id) => request(`/v1/tokens/${encodeURIComponent(id)}`, { method: 'DELETE' }),
}

/** Read a File as data URL for local API upload. */
export function fileToDataUrl(file) {
  return new Promise((resolve, reject) => {
    const r = new FileReader()
    r.onload = () => resolve(r.result)
    r.onerror = reject
    r.readAsDataURL(file)
  })
}
