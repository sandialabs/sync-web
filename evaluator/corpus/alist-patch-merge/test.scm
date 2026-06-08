(define (patch-alist base patch)
  (let loop ((rest patch) (out base))
    (if (null? rest)
        out
        (let* ((entry (car rest))
               (key (car entry))
               (value (cadr entry))
               (without (let remove ((xs out))
                          (cond ((null? xs) '())
                                ((eq? (caar xs) key) (remove (cdr xs)))
                                (else (cons (car xs) (remove (cdr xs))))))))
          (loop (cdr rest)
                (if (equal? value '(nothing))
                    without
                    (cons (list key value) without)))))))

(patch-alist '((a 1) (b 2)) '((b 20) (c 30) (a (nothing))))
