-- Exact Contract and Forma filetype and indentation defaults for Neovim.
-- Drop into ~/.config/nvim/after/plugin/exact-contract.lua or require() from init.lua.

vim.filetype.add({
  extension = {
    contract = "exact-contract",
    forma = "exact-forma",
  },
})

vim.api.nvim_create_autocmd("FileType", {
  pattern = { "exact-contract", "exact-forma" },
  callback = function()
    vim.bo.commentstring = "// %s"
    vim.bo.shiftwidth = 2
    vim.bo.tabstop = 2
    vim.bo.expandtab = true
  end,
})
