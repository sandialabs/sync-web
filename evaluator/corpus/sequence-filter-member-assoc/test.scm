(let ((alist '((alice . 1) (bob . 2) (carol . 3))))
  (list
    (member 'b '(a b c))
    (memq 'b '(a b c))
    (assoc 'bob alist)
    (let loop ((xs '(1 2 3 4 5)) (out '()))
      (if (null? xs)
          (reverse out)
          (loop (cdr xs) (if (odd? (car xs)) (cons (car xs) out) out))))))
