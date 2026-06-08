(list
  (list 'chars #\a #\space #\newline)
  (list 'predicates (char? #\a) (char=? #\a #\a) (char<? #\a #\b))
  (list 'case (char-upcase #\a) (char-downcase #\Z))
  (list 'integer (char->integer #\A) (integer->char 66)))
