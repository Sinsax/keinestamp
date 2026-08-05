export interface StatusFile {
  path: string
  x: string
  y: string
}

export function parseStatus(output: string): StatusFile[] {
  return output
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l.length > 0)
    .map((l) => ({
      x: l[0] || ' ',
      y: l[1] || ' ',
      path: l.substring(3).trim(),
    }))
}
