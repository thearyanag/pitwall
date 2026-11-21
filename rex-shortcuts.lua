-- Pitwall-specific Mac shortcuts. Other programs retain Rex's normal actions.
local terminal = "com.superlogical.terminal"
local function is_sprout(ctx)
  if not ctx.block_id then return false end
  local process = rex.block.call(terminal, "process", { block_id = ctx.block_id })
  return process and process.foreground and process.foreground.name == "sprout"
end
rex.action{
  name = "sprout_paste",
  title = "Paste into Pitwall or Pane",
  run = function(ctx)
    if is_sprout(ctx) then
      local _, err = rex.block.call(terminal, "write", { block_id = ctx.block_id, data = string.char(22) })
      if err then error(err, 0) end
    else
      rex.client.queue("pane.paste")
    end
  end,
}
rex.action{
  name = "sprout_send",
  title = "Send in Pitwall or Split Pane",
  run = function(ctx)
    if is_sprout(ctx) then
      local _, err = rex.block.call(terminal, "write", { block_id = ctx.block_id, data = string.char(19) })
      if err then error(err, 0) end
    else
      rex.client.queue("pane.split.auto")
    end
  end,
}
rex.bind("cmd+v", "sprout_paste")
rex.bind("cmd+enter", "sprout_send")
