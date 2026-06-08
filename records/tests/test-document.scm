(lambda (assertions-src standard-src document-src)

  (eval assertions-src)

  (define standard
    (let ((init (caddr standard-src)))
      (sync-eval ((eval `(lambda* ,(cddadr init) ,@(cddr init))) standard-src) #f)))

  (define document-1 (sync-eval (assert ((standard 'init) document-src #u(104 101 108 108 111)) sync-node?) #f))
  (define document-2 (sync-eval (assert ((standard 'init) document-src #u(1 2 3) '((alpha ((kind "plain"))))) sync-node?) #f))

  (assert ((document-1 '*type*)) 'document)
  (assert (memq 'get (document-1 '*api*)) (lambda (x) x))
  (assert (memq 'set! (document-1 '*api*)) (lambda (x) x))

  (assert ((document-1 'get) 'value) #u(104 101 108 108 111))
  (assert ((document-1 'get) 'meta) '())

  (assert ((document-2 'get) 'value) #u(1 2 3))
  (assert ((document-2 'get) 'meta) '((alpha ((kind "plain")))))

  (assert ((document-1 'set!) 'value #u()) #t)
  (assert ((document-1 'get) 'value) #u())

  (assert (catch #t
                 (lambda () ((document-1 'set!) 'value #f) 'no-error)
                 (lambda args (car args)))
          'value-error)

  (assert ((document-1 'set!) 'meta '((beta ((enabled? #t)
                                             (rank 7))))) #t)
  (assert ((document-1 'get) 'meta) '((beta ((enabled? #t)
                                             (rank 7)))))

  (assert ((document-1 'set!) 'meta '()) #t)
  (assert ((document-1 'get) 'meta) '((beta ((enabled? #t)
                                             (rank 7)))))

  (assert ((document-1 'set!) 'meta '((alpha ((label "sample"))))) #t)
  (assert ((document-1 'get) 'meta) '((alpha ((label "sample")))
                                      (beta ((enabled? #t)
                                             (rank 7)))))

  (assert ((document-1 'set!) 'meta '((beta (nothing)))) #t)
  (assert ((document-1 'get) 'meta) '((alpha ((label "sample")))))

  (assert ((document-1 'set!) 'meta '(nothing)) #t)
  (assert ((document-1 'get) 'meta) '())

  (let* ((node (document-2))
         (digest (sync-digest node)))
    (assert ((document-2 'slice!) 'value) #t)
    (assert (sync-digest (document-2)) digest)
    (assert ((document-2 'get) 'value) #u(1 2 3))
    (assert ((document-2 'get) 'meta) '(unknown)))

  (let* ((document-3 (sync-eval ((standard 'init) document-src #u(104 101 108 108 111) '((alpha ((kind "plain"))))) #f))
         (node (document-3))
         (digest (sync-digest node)))
    (assert ((document-3 'prune!) 'value) #t)
    (assert (sync-digest (document-3)) digest)
    (assert ((document-3 'get) 'value) '(unknown))
    (assert ((document-3 'get) 'meta) '((alpha ((kind "plain"))))))

  `(passed ,asserted assertions))
