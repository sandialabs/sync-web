(list
  (read (open-input-string "|symbol with spaces|"))
  (read (open-input-string "|a\\|b|"))
  (read (open-input-string "||"))
  "line\n tab\t quote\" slash\\"
  "λ"
  (symbol? (read (open-input-string "|symbol with spaces|"))))
