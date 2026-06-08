(lambda (assertions-src root-src standard-src chain-src tree-src ledger-src document-src interface-src)

  (define interface-src
    (let recurse ((src interface-src))
      (cond ((or (not (list? src)) (null? src)) src)
            ((equal? (car src) 'sync-remote)
             `(sync-call (append ,(caddr src) (list (list 'authentication ,(cadr src))))
                         #t
                         (sync-hash (expression->byte-vector ,(cadr src)))))
            ((equal? (car src) 'sync-call)
             (append `(sync-call ,(cadr src) #t) (cdddr src)))
            ((list? (car src))
             (cons (recurse (car src)) (recurse (cdr src))))
            (else (cons (car src) (recurse (cdr src)))))))

  (eval assertions-src)

  (define (journal-id name)
    (sync-hash (expression->byte-vector name)))

  (define (interface-config clear? root-secret interface-secret admins window)
    `((clear? ,clear?)
      (root-secret ,root-secret)
      (interface-secret ,interface-secret)
      (admins ,admins)
      (window ,window)
      (root ,root-src)
      (interface ,interface-secret)
      (name ,interface-secret)))

  (define (journal-install journal admin-secret interface-secret)
    (sync-call `(,interface-src ,(interface-config #t admin-secret interface-secret '() 4)
                                ',standard-src ',chain-src ',tree-src ',ledger-src ',document-src)
               #t journal))

  (define (journal-query journal query)
    (sync-call query #t journal))

  (define* (interface-query journal interface query (identity #f))
    (journal-query journal (append query `((authentication (,@(if identity `((identity ,identity)) '()) (credentials ,interface)))))))

  (define (admin-step journal secret)
    (journal-query journal `(*step* ,secret)))

  (define (bridge-local interface name)
    `((interface ,interface)
      (policy ((publish push) (subscribe pull)))
      (role #f)
      (remote-name ,name)))

  (define interface-1 "http://journal-1.test/interface")
  (define interface-2 "http://journal-2.test/interface")
  (define interface-3 "http://journal-3.test/interface")
  (define interface-4 "http://journal-4.test/interface")
  (define interface-5 "http://journal-5.test/interface")
  (define journal-1 (journal-id interface-1))
  (define journal-2 (journal-id interface-2))
  (define journal-3 (journal-id interface-3))
  (define journal-4 (journal-id interface-4))
  (define journal-5 (journal-id interface-5))

  (sync-create journal-1)
  (sync-create journal-2)
  (sync-create journal-3)
  (sync-create journal-4)
  (sync-create journal-5)

  (assert (journal-install journal-1 "pass-1" interface-1) "Installed interface")
  (assert (journal-install journal-2 "pass-2" interface-2) "Installed interface")
  (assert (journal-install journal-3 "pass-3" interface-3) "Installed interface")
  (assert (journal-install journal-4 "pass-4" interface-4) "Installed interface")
  (assert (journal-install journal-5 "pass-5" interface-5) "Installed interface")

  (let ((query '((function size))))
    (assert (journal-query journal-1 query) 0))

  (let ((query '((function set!) (arguments ((path (*state* hello)) (value "world") (expression? #t))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (let ((query '((function get) (arguments ((expression? #t) (path (*state* hello)))))))
    (assert (interface-query journal-1 interface-1 query) "world"))

  (let ((query '((function set-batch!)
                 (arguments ((paths ((*state* batch alpha) (*state* batch beta)))
                             (values ("a" "b"))
                             (expression? #t)
                             (metas (((alpha ((kind "plain"))))
                                     ((beta #f)))))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (let ((query '((function get) (arguments ((expression? #t) (path (*state* batch alpha)))))))
    (assert (interface-query journal-1 interface-1 query) "a"))

  (let ((query '((function get) (arguments ((expression? #t) (path (*state* batch beta)))))))
    (assert (interface-query journal-1 interface-1 query) "b"))

  (let ((query '((function get) (arguments ((expression? #t) (path (*state* batch alpha)) (meta? #t))))))
    (assert (interface-query journal-1 interface-1 query)
            '((content "a") (meta ((alpha ((kind "plain"))))))))

  (let ((query '((function get) (arguments ((expression? #t) (path (*state* batch beta)) (meta? #t))))))
    (assert (interface-query journal-1 interface-1 query)
            '((content "b") (meta ((beta #f))))))

  (let ((query '((function get) (arguments ((path (*transition* operation)))))))
    (assert (interface-query journal-1 interface-1 query) '((path (*state* batch beta)) (value "b") (expression? #t) (meta ((beta #f))))))

  (let ((query '((function get) (arguments ((path (*transition* previous operation)))))))
    (assert (interface-query journal-1 interface-1 query)
            '((path (*state* batch alpha)) (value "a") (expression? #t) (meta ((alpha ((kind "plain"))))))))

  (let ((query '((function get) (arguments ((path (*transition* previous previous operation)))))))
    (assert (interface-query journal-1 interface-1 query) '((path (*state* hello)) (value "world") (expression? #t))))

  (let ((query '((function set!) (arguments ((path (*state* bytes doc)) (value #u(4 5 6)))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (let ((query '((function get) (arguments ((path (*state* bytes doc)))))))
    (assert (interface-query journal-1 interface-1 query) #u(4 5 6)))

  (let ((query '((function set!) (arguments ((path (*state* bytes rejected)) (value "not bytes"))))))
    (assert (interface-query journal-1 interface-1 query) (lambda (x) (eq? (car x) 'error))))

  (assert (admin-step journal-1 "pass-1") 1)

  (let ((query '((function resolve) (arguments ((expression? #t) (path (-1 *state* hello)) (pinned? #f) (proof? #f))))))
    (assert (interface-query journal-1 interface-1 query) "world"))

  (let ((query '((function set!) (arguments ((path (*state* do pin this)) (value "yes") (expression? #t))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (let ((query '((function set!) (arguments ((path (*state* do pin that)) (value "yes") (expression? #t))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (let ((query '((function set!) (arguments ((path (*state* do not pin)) (value "no") (expression? #t))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (assert (admin-step journal-1 "pass-1") 2)

  (let ((query '((function pin!) (arguments ((path (1 *state* do pin this)))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (let ((query '((function pin!) (arguments ((path (1 *state* do pin that)))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (let ((query `((function bridge!) (arguments ((name journal-2) (info-local ,(bridge-local interface-2 'journal-2)))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (let ((query '((function set!) (arguments ((path (*state* a b c)) (value 42) (expression? #t))))))
    (assert (interface-query journal-2 interface-2 query) #t))

  (assert (admin-step journal-2 "pass-2") 1)

  (assert (admin-step journal-1 "pass-1") 3)

  (let ((query '((function resolve) (arguments ((path (-1 *transition* previous previous operation)))))))
    (assert (interface-query journal-1 interface-1 query)
            (lambda (x) (and (equal? (cadr (assoc 'path x)) '(*bridge* journal-2))
                             (equal? (cadr (assoc 'valid? (cadr (assoc 'value x)))) #t)
                             (integer? (cadr (assoc 'index (cadr (assoc 'value x)))))))))

  (let* ((path '(-1 *bridge* journal-2 *state* a b c))
         (query `((function resolve) (arguments ((expression? #t) (path ,path) (pinned? #f) (proof? #f))))))
    (assert (interface-query journal-1 interface-1 query) 42))

  (let ((query '((function resolve) (arguments ((expression? #t) (path (-1 *bridge*)) (pinned? #f) (proof? #f))))))
    (assert (interface-query journal-1 interface-1 query) '(directory ((journal-2 directory)) #t)))

  (let* ((path '(-1 *bridge* journal-2 *state* a b))
         (query `((function resolve) (arguments ((expression? #t) (path ,path) (pinned? #f) (proof? #f))))))
    (assert (interface-query journal-1 interface-1 query) '(directory ((c value)) #t)))

  (let ((query `((function bridge!) (arguments ((name journal-3) (info-local ,(bridge-local interface-3 'journal-3)))))))
    (assert (interface-query journal-2 interface-2 query) #t))

  (let ((query `((function bridge!) (arguments ((name journal-4) (info-local ,(bridge-local interface-4 'journal-4)))))))
    (assert (interface-query journal-3 interface-3 query) #t))

  (let ((query `((function bridge!) (arguments ((name journal-5) (info-local ,(bridge-local interface-5 'journal-5)))))))
    (assert (interface-query journal-3 interface-3 query) #t))

  (let ((query '((function set!) (arguments ((path (*state* d e f)) (value 64) (expression? #t))))))
    (assert (interface-query journal-3 interface-3 query) #t))

  (let ((query '((function set!) (arguments ((path (*state* g h i)) (value "hello") (expression? #t))))))
    (assert (interface-query journal-4 interface-4 query) #t))

  (let ((query '((function set!) (arguments ((path (*state* g h i)) (value "world") (expression? #t))))))
    (assert (interface-query journal-5 interface-5 query) #t))

  (assert (admin-step journal-3 "pass-3") 1)

  (assert (admin-step journal-4 "pass-4") 1)

  (assert (admin-step journal-5 "pass-5") 1)

  (assert (admin-step journal-3 "pass-3") 2)

  (assert (admin-step journal-2 "pass-2") 2)

  (assert (admin-step journal-1 "pass-1") 4)

  (let* ((path '(-1 *bridge* journal-3 *state* d e f))
         (query `((function resolve) (arguments ((expression? #t) (path ,path) (pinned? #f) (proof? #f))))))
    (assert (interface-query journal-2 interface-2 query) 64))

  (assert (admin-step journal-2 "pass-2") 2)

  (assert (admin-step journal-1 "pass-1") 4)

  (let* ((path '(-1 *bridge* journal-2 -1 *bridge* journal-3 *state* d e f))
         (query `((function resolve) (arguments ((expression? #t) (path ,path) (pinned? #f) (proof? #f))))))
    (assert (interface-query journal-1 interface-1 query) 64))

  (assert (admin-step journal-2 "pass-2") 2)

  (assert (admin-step journal-1 "pass-1") 4)

  (let* ((path '(-1 *bridge* journal-2 -1 *bridge* journal-3 -1 *bridge* journal-4 *state* g h i))
         (query `((function resolve) (arguments ((expression? #t) (path ,path) (pinned? #f) (proof? #f))))))
    (assert (interface-query journal-1 interface-1 query) "hello"))

  (let* ((path '(-1 *bridge* journal-2 -1 *bridge* journal-3 -1 *bridge* journal-5 *state* g h i))
         (query `((function resolve) (arguments ((expression? #t) (path ,path) (pinned? #t) (proof? #f))))))
    (assert (interface-query journal-1 interface-1 query) (lambda (x) (equal? (cadr (assoc 'content x)) "world"))))

  (let ((query '((function set!) (arguments ((path (*state* tick)) (value 0) (expression? #t))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (assert (admin-step journal-1 "pass-1") 5)

  (let ((query '((function set!) (arguments ((path (*state* tick)) (value 1) (expression? #t))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (assert (admin-step journal-1 "pass-1") 6)

  (let ((query '((function set!) (arguments ((path (*state* tick)) (value 2) (expression? #t))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (assert (admin-step journal-1 "pass-1") 7)

  (let ((query '((function set!) (arguments ((path (*state* tick)) (value 3) (expression? #t))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (assert (admin-step journal-1 "pass-1") 8)

  (let* ((path '(6 *bridge* journal-2 *state* a b c))
         (pin-query `((function pin!) (arguments ((path ,path)))))
         (resolve-query `((function resolve) (arguments ((expression? #t) (path ,path) (pinned? #t) (proof? #f))))))
    (assert (interface-query journal-1 interface-1 pin-query) #t)
    (assert (interface-query journal-1 interface-1 resolve-query)
            (lambda (res) (cadr (assoc 'pinned? res)))))

  (let ((query '((function resolve) (arguments ((expression? #t) (path (1 *state* do pin)) (pinned? #f) (proof? #f))))))
    (assert (interface-query journal-1 interface-1 query) '(directory ((this value) (that value)) #t)))

  (let ((query '((function resolve) (arguments ((expression? #t) (path (1 *state* do pin this)) (pinned? #f) (proof? #f))))))
    (assert (interface-query journal-1 interface-1 query) "yes"))

  (let ((query '((function resolve) (arguments ((expression? #t) (path (1 *state* do pin that)) (pinned? #f) (proof? #f))))))
    (assert (interface-query journal-1 interface-1 query) "yes"))

  (let ((query '((function resolve) (arguments ((expression? #t) (path (1 *state* do not pin)) (pinned? #f) (proof? #f))))))
    (assert (interface-query journal-1 interface-1 query) '(unknown)))

  (let ((query '((function unpin!) (arguments ((path (1 *state* do pin that)))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (let ((query '((function resolve) (arguments ((expression? #t) (path (1 *state* do pin this)) (pinned? #f) (proof? #f))))))
    (assert (interface-query journal-1 interface-1 query) "yes"))

  (let ((query '((function resolve) (arguments ((expression? #t) (path (1 *state* do pin that)) (pinned? #f) (proof? #f))))))
    (assert (interface-query journal-1 interface-1 query) '(unknown)))

  (let ((query '((function *secret*) (arguments ((secret "pass-1-new"))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (let* ((expr `(,interface-src ,(interface-config #f "pass-1" interface-1 '() 2)
                                ',standard-src ',chain-src ',tree-src ',ledger-src ',document-src))
         (query `(*eval* "pass-1" ,expr)))
    (assert (journal-query journal-1 query) "Installed interface"))

  (let ((query '((function size))))
    (assert (journal-query journal-1 query) 8))

  (let ((query '((function config) (arguments ((path (public window)))))))
    (assert (interface-query journal-1 interface-1 query) 2))

  (let ((query '((function get) (arguments ((expression? #t) (path (*state* hello)))))))
    (assert (interface-query journal-1 interface-1 query) "world"))

  (let ((query '((function resolve) (arguments ((expression? #t) (path (-1 *state* hello)) (pinned? #f) (proof? #f))))))
    (assert (interface-query journal-1 interface-1 query) "world"))

  (let ((query '((function pin!) (arguments ((path (-1 *state* hello)))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (let ((query '((function resolve) (arguments ((expression? #t) (path (-1 *state* hello)) (pinned? #t) (proof? #f))))))
    (assert (interface-query journal-1 interface-1 query) '((content "world") (pinned? #t))))

  (let ((query '((function unpin!) (arguments ((path (-1 *state* hello)))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (let ((query '((function set!) (arguments ((path (*state* alice data)) (value "public data") (expression? #t))))))
    (assert (interface-query journal-1 interface-1 query 'alice) #t))

  (let ((query '((function set!) (arguments ((path (*state* alice *private* data)) (value "private data") (expression? #t))))))
    (assert (interface-query journal-1 interface-1 query 'alice) #t))

  (let ((query '((function set!) (arguments ((path (*state* alice data)) (value "bob's data") (expression? #t))))))
    (assert (interface-query journal-1 interface-1 query 'bob) (lambda (x) (eq? (car x) 'error))))

  (let ((query '((function get) (arguments ((expression? #t) (path (*state* alice data)))))))
    (assert (interface-query journal-1 interface-1 query 'alice) "public data"))

  (let ((query '((function get) (arguments ((expression? #t) (path (*state* alice *private* data)))))))
    (assert (interface-query journal-1 interface-1 query 'alice) "private data"))

  (let ((query '((function get) (arguments ((expression? #t) (path (*state* alice *private* data)))))))
    (assert (interface-query journal-1 interface-1 query 'bob) (lambda (x) (eq? (car x) 'error))))

  (let ((query '((function set!) (arguments ((path (*state* alice foo *private*)) (value "my private data") (expression? #t))))))
    (assert (interface-query journal-1 interface-1 query 'alice) #t))

  ; cross-user public read — bob can read alice's non-private data
  (let ((query '((function get) (arguments ((expression? #t) (path (*state* alice data)))))))
    (assert (interface-query journal-1 interface-1 query 'bob) "public data"))

  ; non-admin users can read the ownerless state root as public directory metadata
  (let ((query '((function get) (arguments ((expression? #t) (path (*state*)))))))
    (assert (interface-query journal-1 interface-1 query 'bob) (lambda (x) (and (list? x) (eq? (car x) 'directory)))))

  (let ((query '((function resolve) (arguments ((expression? #t) (path (-1 *state*)) (pinned? #t) (proof? #t))))))
    (assert (interface-query journal-1 interface-1 query 'bob) (lambda (x) (and (list? x) (assoc 'content x)))))

  ; set-batch! with paths from mixed owners — fails when any path is not owned by identity
  (let ((query '((function set-batch!)
                 (arguments ((paths ((*state* bob stuff) (*state* alice stuff)))
                             (values ("val1" "val2")) (expression? #t))))))
    (assert (interface-query journal-1 interface-1 query 'bob) (lambda (x) (eq? (car x) 'error))))

  ; non-admin user calling bridge! — operation requires admin privileges
  (let ((query `((function bridge!) (arguments ((name journal-2) (info-local ,(bridge-local interface-2 'journal-2)))))))
    (assert (interface-query journal-1 interface-1 query 'alice) (lambda (x) (eq? (car x) 'error))))

  ; *admins-get* with root identity returns current (empty) admin list
  (let ((query '((function *admins-get*))))
    (assert (interface-query journal-1 interface-1 query) '()))

  ; *admins-set* promotes alice to admin
  (let ((query '((function *admins-set*) (arguments ((admins (alice)))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  ; promoted alice can now call *admins-get* — confirms admin list is enforced
  (let ((query '((function *admins-get*))))
    (assert (interface-query journal-1 interface-1 query 'alice) '(alice)))

  ; admins can update the public window through the interface-level admin operation
  (let ((query '((function *window-set*) (arguments ((value 3))))))
    (assert (interface-query journal-1 interface-1 query 'alice) #t))

  (let ((query '((function config) (arguments ((path (public window)))))))
    (assert (interface-query journal-1 interface-1 query 'alice) 3))

  ; non-positive window sizes are rejected before reaching ledger config updates
  (let ((query '((function *window-set*) (arguments ((value 0))))))
    (assert (interface-query journal-1 interface-1 query 'alice) (lambda (x) (eq? (car x) 'error))))

  (let ((query '((function set!)
                 (arguments ((path (*state* metadata doc))
                             (value "hello") (expression? #t)
                             (meta ((alpha ((kind "plain"))))))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (let ((query '((function get) (arguments ((expression? #t) (path (*state* metadata doc)) (meta? #t))))))
    (assert (interface-query journal-1 interface-1 query)
            '((content "hello") (meta ((alpha ((kind "plain"))))))))

  (let ((query '((function set!)
                 (arguments ((path (*state* metadata doc))
                             (meta ((beta #f))))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (let ((query '((function get) (arguments ((expression? #t) (path (*state* metadata doc)) (meta? #t))))))
    (assert (interface-query journal-1 interface-1 query)
            '((content "hello") (meta ((beta #f) (alpha ((kind "plain"))))))))

  (let ((query '((function set!)
                 (arguments ((path (*state* metadata doc))
                             (meta ((beta (nothing)))))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (let ((query '((function set!)
                 (arguments ((path (*state* metadata false-value))
                             (value #f) (expression? #t))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (let ((query '((function get) (arguments ((expression? #t) (path (*state* metadata false-value)))))))
    (assert (interface-query journal-1 interface-1 query) (lambda (x) (eq? x #f))))

  (let ((query '((function set!)
                 (arguments ((path (*state* metadata missing))
                             (meta ((alpha ((kind "plain"))))))))))
    (assert (interface-query journal-1 interface-1 query) (lambda (x) (eq? (car x) 'error))))

  (let ((query '((function set!)
                 (arguments ((path (*state* metadata doc))
                             (meta (nothing)))))))
    (assert (interface-query journal-1 interface-1 query) #t))

  (let ((query '((function get) (arguments ((expression? #t) (path (*state* metadata doc)) (meta? #t))))))
    (assert (interface-query journal-1 interface-1 query) '((content "hello") (meta ()))))

  (append "Success (" (object->string asserted) " checks)"))
