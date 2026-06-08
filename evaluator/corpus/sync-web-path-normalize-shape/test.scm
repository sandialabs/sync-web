(define (split-public-path path)
  (let loop ((rest path) (indexes '()) (tail '()))
    (cond ((null? rest) (list (reverse indexes) (reverse tail)))
          ((integer? (car rest)) (loop (cdr rest) (cons (car rest) indexes) tail))
          (else (loop (cdr rest) indexes (cons (car rest) tail))))))

(list
  (split-public-path '(-1 *state* alice doc))
  (split-public-path '(-1 *bridge* peer -2 *state* bob doc)))
