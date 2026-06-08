(let ((xs '(b c))
      (tail '(d . e))
      (n 3))
  (list
    `(a ,(+ 1 2) ,@xs . ,tail)
    `(outer `(inner ,n ,',n))
    '((a . b) c . d)
    (let ((p '(left . right))) (list (car p) (cdr p)))))
