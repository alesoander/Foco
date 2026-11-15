-- ── Foco · reglas para Hyprland (configuración Lua) ─────────────
-- Copia esto a tu configuración de Hyprland (o cárgalo con dofile/require).
-- Sintaxis comprobada en wiki.hypr.land (versión en desarrollo, nov. 2026).
-- Si usas una versión con el formato .conf antiguo, mira hyprland.conf.

-- Widget: flotante, fijado en todos los escritorios (pin = siempre visible),
-- sin robar el foco al aparecer, arriba a la derecha.
hl.window_rule({
  name             = "foco-widget",
  match            = { title = "^Foco Widget$" },
  float            = true,
  pin              = true,
  -- Sin regla de tamaño: Foco lo fija según el ajuste "Tamaño del widget".
  move             = { "monitor_w - window_w - 20", "56" },
  no_initial_focus = true,
  no_dim           = true,
  no_follow_mouse  = true,
  border_size      = 0,
  rounding         = 18,
})

-- Ventana principal flotante y centrada (opcional: bórralo si la prefieres en mosaico)
hl.window_rule({
  name     = "foco-main",
  match    = { title = "^Foco$" },
  float    = true,
  size     = { 980, 660 },
  center   = true,
  rounding = 12,
})

-- Vidrio: el blur del compositor desenfoca lo que hay detrás de las ventanas
-- transparentes (viene activado por defecto).
hl.config({ ["decoration.blur.enabled"] = true })

-- Super+T alterna ventana ↔ widget
hl.bind("SUPER + T", hl.dsp.exec_cmd("foco --toggle"))

-- Arrancar como widget al iniciar sesión
hl.on("hyprland.start", function()
  hl.exec_cmd("foco --widget")
end)

-- Solo si tienes NVIDIA y ves la ventana en blanco:
-- hl.env("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
