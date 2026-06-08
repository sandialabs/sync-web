(define (remove-key alist key)
  (cond ((null? alist) '())
        ((eq? (caar alist) key) (remove-key (cdr alist) key))
        (else (cons (car alist) (remove-key (cdr alist) key)))))

(define (metadata-patch meta patch)
  (cond ((null? patch) meta)
        ((equal? patch '(nothing)) '())
        (else
          (let loop ((rest patch) (out meta))
            (if (null? rest)
                out
                (let* ((key (caar rest))
                       (value (cadar rest))
                       (without (remove-key out key)))
                  (loop (cdr rest)
                        (if (equal? value '(nothing))
                            without
                            (cons (list key value) without)))))))))

(list
  (metadata-patch '((mime "text/plain") (owner alice)) '())
  (metadata-patch '((mime "text/plain") (owner alice)) '((owner bob) (etag "abc")))
  (metadata-patch '((mime "text/plain") (owner alice)) '((owner (nothing))))
  (metadata-patch '((mime "text/plain") (owner alice)) '(nothing)))
