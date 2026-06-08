(list
  (format #f "~A/~S" "x y" "x y")
  (format #f "~{~A~^, ~}" '(a b c))
  (format #f "~D ~X ~O ~B" 31 31 31 31)
  (format #f "~~ ~% done"))
