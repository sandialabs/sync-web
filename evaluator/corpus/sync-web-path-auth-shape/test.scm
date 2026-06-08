(define (starts-with? xs prefix)
  (cond ((null? prefix) #t)
        ((null? xs) #f)
        ((equal? (car xs) (car prefix)) (starts-with? (cdr xs) (cdr prefix)))
        (else #f)))

(define (private-path? path)
  (and (pair? path) (eq? (car path) '*private*)))

(define (own-state-path? identity path)
  (starts-with? path (list '*state* identity)))

(define (authorized? admin? identity op path)
  (cond (admin? #t)
        ((and (eq? op 'write) (own-state-path? identity path)) #t)
        ((and (eq? op 'read)
              (not (and (starts-with? path '(*state*))
                        (pair? (cdr path))
                        (not (eq? (cadr path) identity))
                        (private-path? (cddr path)))))) #t)
        (else #f)))

(list
  (authorized? #f 'alice 'write '(*state* alice docs note))
  (authorized? #f 'alice 'write '(*state* bob docs note))
  (authorized? #f 'alice 'read '(*state* bob *private* token))
  (authorized? #f 'alice 'read '(*state* bob public note))
  (authorized? #t 'alice 'write '(*config* admins)))
